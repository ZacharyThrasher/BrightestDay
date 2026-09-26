"""Pokemon moves (PokeAPI moves.csv) -> closed-set graft data for bd_lexicon.

Vocabulary vs. mechanics: a move is only ever *which closed-set mechanics / element / temperament it
means* (plus a construct form for when it is cast on its own). What those tags do is decided by bd_sim.

Rules (in order): FAMOUS hand table > name keywords > PokeAPI meta (ailment / category / drain /
priority / multi-hit) > damage class default (physical -> Blunt, special -> Shoots, status -> skipped).
The construct form (cast alone, nothing aimed) is derived from the mechanics: a Beam move is a Staff
emitter, a punch a Gauntlet, a whip a Whip, Explodes a Bomb... Its mechanics are EXACTLY the graft
mechanics (the sim grafts a recipe by merging its mechanics), so casting "thunderbolt" at your own
Pikachu adds Beam + Electric and nothing else.

Output of `build(norm, csv_dir)`: list of dicts
    k (normalised name), d (display), f (form), m (mechanics), e (element or None), t (temperament or None),
    s (scale), w (mass), famous (bool)
"""
import csv
import re
from pathlib import Path

# type -> element (same spirit as anchors.POKEMON_TYPE; moves read the type literally)
TYPE_ELEMENT = {
    "normal": None, "fire": "Fire", "water": "Water", "electric": "Electric", "grass": "Earth", "ice": "Ice",
    "fighting": None, "poison": "Poison", "ground": "Earth", "flying": "Wind", "psychic": "Psychic",
    "bug": "Poison", "rock": "Earth", "ghost": "Shadow", "dragon": "Fire", "dark": "Shadow", "steel": None,
    "fairy": "Psychic", "shadow": "Shadow", "stellar": "Light",
}

# Famous moves, hand-mapped: key -> (mechanics, temperament or None, form or None, element override or None)
# element None = from the move's type. "" temperament = none.
FAMOUS = {
    "thunderbolt": (["Beam"], None, "Staff", None),
    "thunder shock": (["Shoots"], None, "Staff", None),
    "thunder": (["DropFromSky"], None, "Staff", None),
    "thunder wave": (["Repel"], None, "Staff", None),
    "thunder punch": (["Blunt"], None, "Gauntlet", None),
    "volt tackle": (["Charge", "Fast"], None, "Spear", None),
    "flamethrower": (["Breath"], None, "Staff", None),
    "fire blast": (["Shoots", "Explodes"], None, "Staff", None),
    "ember": (["Shoots"], None, "Staff", None),
    "fire spin": (["Spin", "Trap"], None, "Cage", None),
    "blast burn": (["Beam", "Explodes"], None, "Cannon", None),
    "hydro pump": (["Beam", "Repel"], None, "Cannon", None),
    "hydro cannon": (["Beam", "Heavy"], None, "Cannon", None),
    "water gun": (["Shoots"], None, "Gun", None),
    "bubble beam": (["Beam"], None, "Staff", None),
    "surf": (["Repel", "Charge"], None, "Wall", None),
    "waterfall": (["Charge", "Launch"], None, "Spear", None),
    "whirlpool": (["Spin", "Trap"], None, "Cage", None),
    "hyper beam": (["Beam", "Heavy"], None, "Cannon", None),
    "giga impact": (["Charge", "Heavy"], None, "Spear", None),
    "solar beam": (["Beam"], None, "Staff", None),
    "ice beam": (["Beam"], None, "Staff", None),
    "blizzard": (["Breath", "Repel"], None, "Staff", None),
    "psybeam": (["Beam"], None, "Staff", None),
    "psychic": (["Beam"], None, "Staff", None),
    "shadow ball": (["Shoots"], None, "Ball", None),
    "aura sphere": (["Shoots", "Homing"], None, "Ball", None),
    "focus blast": (["Shoots", "Explodes"], None, "Ball", None),
    "dragon pulse": (["Beam", "Repel"], None, "Staff", None),
    "dragon rage": (["Breath"], None, "Staff", None),
    "dragon breath": (["Breath"], None, "Staff", None),
    "draco meteor": (["DropFromSky", "Explodes"], None, "Boulder", None),
    "outrage": (["Charge"], "Aggressive", "Spear", None),
    "earthquake": (["Stomp"], None, "Hammer", None),
    "magnitude": (["Stomp"], None, "Hammer", None),
    "bulldoze": (["Stomp"], None, "Hammer", None),
    "fissure": (["Stomp", "Heavy"], None, "Hammer", None),
    "dig": (["Launch"], None, "Pillar", "Earth"),
    "rock slide": (["DropFromSky"], None, "Boulder", None),
    "rock throw": (["Shoots", "Heavy"], None, "Boulder", None),
    "stone edge": (["Pierce", "Launch"], None, "Pillar", None),
    "rollout": (["Spin", "Charge"], None, "Boulder", None),
    "fly": (["Flies"], None, "Wings", None),
    "sky attack": (["Flies", "Charge"], None, "Wings", None),
    "brave bird": (["Flies", "Charge"], None, "Wings", None),
    "wing attack": (["Blade", "Flies"], None, "Wings", None),
    "aerial ace": (["Blade", "Homing"], None, "Sword", None),
    "gust": (["Repel"], None, "Staff", None),
    "whirlwind": (["Repel", "Spin"], None, "Pillar", None),
    "hurricane": (["Repel", "Spin"], None, "Pillar", None),
    "twister": (["Spin", "Repel"], None, "Pillar", None),
    "roar": (["Repel"], "Aggressive", "Staff", None),
    "explosion": (["Explodes"], None, "Bomb", None),
    "self destruct": (["Explodes"], None, "Bomb", None),
    "misty explosion": (["Explodes"], None, "Bomb", None),
    "quick attack": (["Fast", "Charge"], None, "Spear", None),
    "extreme speed": (["Fast", "Charge"], None, "Spear", None),
    "agility": (["Fast"], None, None, None),
    "tackle": (["Charge"], None, "Spear", None),
    "take down": (["Charge", "Heavy"], None, "Spear", None),
    "double edge": (["Charge", "Heavy"], None, "Spear", None),
    "body slam": (["Heavy", "DropFromSky"], None, "Anvil", None),
    "heavy slam": (["Heavy", "DropFromSky"], None, "Anvil", None),
    "slam": (["Blunt", "Heavy"], None, "Hammer", None),
    "headbutt": (["Charge", "Blunt"], None, "Spear", None),
    "skull bash": (["Charge", "Blunt"], None, "Spear", None),
    "mega punch": (["Blunt", "Heavy"], None, "Gauntlet", None),
    "mega kick": (["Blunt", "Launch"], None, "Gauntlet", None),
    "hi jump kick": (["Launch", "Blunt"], None, "Gauntlet", None),
    "high jump kick": (["Launch", "Blunt"], None, "Gauntlet", None),
    "close combat": (["Blunt", "Fast"], "Aggressive", "Gauntlet", None),
    "seismic toss": (["Grab", "Launch"], None, "Gauntlet", None),
    "submission": (["Grab"], None, "Gauntlet", None),
    "vine whip": (["Pull"], None, "Whip", None),
    "power whip": (["Pull", "Heavy"], None, "Whip", None),
    "razor leaf": (["Shoots", "Blade"], None, "Disc", None),
    "leaf blade": (["Blade"], None, "Sword", None),
    "leech seed": (["Sticky"], None, "Ball", None),
    "sleep powder": (["Breath"], "Lazy", "Staff", None),
    "stun spore": (["Breath"], None, "Staff", "Electric"),
    "poison powder": (["Breath"], None, "Staff", "Poison"),
    "string shot": (["Sticky"], None, "Whip", None),
    "wrap": (["Constrict"], None, "Whip", None),
    "bind": (["Constrict"], None, "Whip", None),
    "iron tail": (["Blunt", "Heavy"], None, "Whip", None),
    "dragon tail": (["Blunt", "Repel"], None, "Whip", None),
    "iron defense": (["Guard"], None, "Shield", None),
    "protect": (["Guard"], None, "Shield", None),
    "detect": (["Guard"], None, "Shield", None),
    "reflect": (["Guard"], None, "Shield", None),
    "light screen": (["Barrier"], None, "Wall", None),
    "barrier": (["Barrier"], None, "Wall", None),
    "withdraw": (["Guard"], None, "Shield", None),
    "harden": (["Guard"], None, "Shield", None),
    "teleport": (["Portal"], None, "Wall", None),
    "minimize": (["Shrink"], None, "Staff", None),
    "growth": (["Grow"], None, "Staff", None),
    "double team": (["Swarm"], None, None, None),
    "swords dance": ([], "Aggressive", "Sword", None),
    "dragon dance": (["Fast"], "Aggressive", None, None),
    "bulk up": ([], "Aggressive", "Gauntlet", None),
    "nasty plot": ([], "Sneaky", None, None),
    "calm mind": ([], "Calm", None, None),
    "rest": ([], "Lazy", None, None),
    "yawn": ([], "Lazy", None, None),
    "amnesia": ([], "Lazy", None, None),
    "sing": (["Repel"], "Lazy", "Staff", None),
    "hypnosis": (["Repel"], "Lazy", "Staff", None),
    "lovely kiss": ([], "Lazy", None, None),
    "confuse ray": (["Homing"], "Chaotic", "Ball", None),
    "metronome": ([], "Chaotic", None, None),
    "splash": ([], "Playful", None, None),       # does nothing — a harmless funny flop
    "attract": (["Attract"], "Playful", "Ball", None),
    "charm": ([], "Playful", None, None),
    "tail whip": ([], "Playful", None, None),
    "helping hand": ([], "Protective", None, None),
    "follow me": ([], "Protective", None, None),
    "wish": ([], "Protective", None, None),
    "recover": ([], "Protective", None, None),
    "shadow sneak": (["Fast"], "Sneaky", "Sword", None),
    "sucker punch": (["Fast"], "Sneaky", "Gauntlet", None),
    "fake out": (["Fast"], "Sneaky", "Gauntlet", None),
    "phantom force": (["Portal"], "Sneaky", "Wall", None),
    "shadow claw": (["Blade"], None, "Gauntlet", None),
    "night slash": (["Blade"], "Sneaky", "Sword", None),
    "bite": (["Grab"], None, "Gauntlet", None),
    "crunch": (["Grab"], None, "Gauntlet", None),
    "hyper fang": (["Grab"], None, "Gauntlet", None),
    "lick": (["Sticky"], None, "Gauntlet", None),
    "swift": (["Shoots", "Homing"], None, "Staff", None),
    "pin missile": (["Shoots", "Homing"], None, "Gun", None),
    "will o wisp": (["Shoots", "Homing"], None, "Staff", None),
    "toxic": (["Breath"], None, "Staff", None),
    "sludge bomb": (["Explodes", "Sticky"], None, "Bomb", None),
    "hail": (["DropFromSky"], None, "Boulder", "Ice"),
    "sandstorm": (["Spin", "Repel"], None, "Pillar", "Earth"),
    "rain dance": ([], None, None, "Water"),
    "sunny day": ([], None, None, "Fire"),
    "spikes": (["Trap"], None, "Cage", None),
    "stealth rock": (["Trap"], None, "Cage", "Earth"),
    "rapid spin": (["Spin"], None, "Disc", None),
    "horn drill": (["Spin", "Pierce"], None, "Spear", None),
    "drill run": (["Spin", "Pierce"], None, "Spear", None),
    "megahorn": (["Pierce", "Charge"], None, "Spear", None),
    "guillotine": (["Blade", "Heavy"], None, "Axe", None),
    "cut": (["Blade"], None, "Sword", None),
    "slash": (["Blade"], None, "Sword", None),
    "fury swipes": (["Blade", "Fast"], None, "Gauntlet", None),
    "metal claw": (["Blade"], None, "Gauntlet", None),
    "dragon claw": (["Blade"], None, "Gauntlet", None),
    "x scissor": (["Blade"], None, "Sword", None),
    "fury cutter": (["Blade", "Fast"], None, "Sword", None),
    "psycho cut": (["Blade", "Shoots"], None, "Sword", None),
    "sacred sword": (["Blade"], None, "Sword", None),
    "water shuriken": (["Shoots", "Blade"], None, "Disc", None),
    "strength": (["Blunt", "Heavy"], None, "Gauntlet", None),
    "rock smash": (["Blunt"], None, "Gauntlet", None),
    "bone club": (["Blunt"], None, "Hammer", None),
    "bonemerang": (["Returning", "Blunt"], None, "Disc", None),
    "wood hammer": (["Blunt", "Heavy"], None, "Hammer", None),
    "hammer arm": (["Blunt", "Heavy"], None, "Hammer", None),
    "meteor mash": (["Blunt", "DropFromSky"], None, "Gauntlet", None),
    "gyro ball": (["Spin", "Heavy"], None, "Ball", None),
    "flash cannon": (["Beam"], None, "Cannon", None),
    "zap cannon": (["Shoots", "Heavy"], None, "Cannon", None),
    "charge beam": (["Beam"], None, "Staff", None),
    "signal beam": (["Beam"], None, "Staff", None),
    "aurora beam": (["Beam"], None, "Staff", None),
    "dark pulse": (["Repel"], None, "Staff", None),
    "water pulse": (["Repel"], None, "Staff", None),
    "hyper voice": (["Repel"], None, "Staff", None),
    "boomburst": (["Repel", "Heavy"], None, "Staff", None),
    "heat wave": (["Breath", "Repel"], None, "Staff", None),
    "overheat": (["Breath", "Heavy"], None, "Staff", None),
    "eruption": (["Explodes", "DropFromSky"], None, "Boulder", None),
    "inferno": (["Breath"], None, "Staff", None),
    "flare blitz": (["Charge", "Heavy"], None, "Spear", None),
    "flame wheel": (["Spin", "Charge"], None, "Disc", None),
    "wild charge": (["Charge"], None, "Spear", None),
    "spark": (["Charge"], None, "Spear", None),
    "discharge": (["Repel"], None, "Dome", None),
    "electro ball": (["Shoots"], None, "Ball", None),
    "energy ball": (["Shoots"], None, "Ball", None),
    "mud slap": (["Shoots"], None, "Staff", None),
    "sand attack": (["Breath"], None, "Staff", None),
    "smokescreen": (["Breath"], None, "Staff", "Shadow"),
    "leaf storm": (["Shoots", "Spin"], None, "Staff", None),
    "petal dance": (["Spin"], "Aggressive", "Disc", None),
    "giga drain": (["Pull"], None, "Whip", None),
    "absorb": (["Pull"], None, "Whip", None),
    "mega drain": (["Pull"], None, "Whip", None),
    "moonblast": (["Shoots", "Explodes"], None, "Ball", None),
    "dazzling gleam": (["Repel"], None, "Dome", None),
    "play rough": (["Blunt"], "Playful", "Gauntlet", None),
    "u turn": (["Returning"], None, "Disc", None),
    "transform": (["Transforms"], None, None, None),
    "substitute": (["Barrier"], None, "Wall", None),
    "counter": (["Guard"], None, "Shield", None),
    "mirror coat": (["Guard"], None, "Shield", None),
    "gravity": (["Attract"], None, "Ball", None),
    "magnet rise": (["Flies"], None, "Wings", None),
    "bounce": (["Bounce", "Flies"], None, "Trampoline", None),
    "high horsepower": (["Stomp", "Charge"], None, "Hammer", None),
    "stomp": (["Stomp"], None, "Hammer", None),
    "stomping tantrum": (["Stomp"], "Aggressive", "Hammer", None),
    "precipice blades": (["Stomp", "Pierce"], None, "Pillar", None),
    "origin pulse": (["Repel", "Beam"], None, "Staff", None),
    "dragon ascent": (["Flies", "Charge"], None, "Wings", None),
    "roar of time": (["Beam", "Heavy"], None, "Cannon", "Atomic"),
    "spacial rend": (["Blade", "Portal"], None, "Sword", None),
    "shadow force": (["Portal", "Charge"], "Sneaky", "Wall", None),
    "psystrike": (["Beam", "Pierce"], None, "Staff", None),
    "judgment": (["DropFromSky", "Shoots"], None, "Staff", None),
    "sky drop": (["Grab", "Flies"], None, "Gauntlet", None),
    "vital throw": (["Grab", "Homing"], None, "Gauntlet", None),
    "circle throw": (["Grab", "Repel"], None, "Gauntlet", None),
    "mean look": (["Trap"], None, "Cage", None),
    "block": (["Barrier"], None, "Wall", None),
    "spider web": (["Trap", "Sticky"], None, "Cage", None),
    "sticky web": (["Sticky", "Trap"], None, "Cage", None),
    "ingrain": ([], "Lazy", None, None),
    "snore": (["Repel"], "Lazy", "Staff", None),
    "belly drum": ([], "Aggressive", None, None),
    "focus energy": ([], "Aggressive", None, None),
    "scary face": ([], "Aggressive", None, None),
    "leer": ([], "Aggressive", None, None),
    "growl": ([], "Aggressive", None, None),
    "screech": (["Repel"], None, "Staff", None),
    "supersonic": (["Repel"], "Chaotic", "Staff", None),
    "swagger": ([], "Chaotic", None, None),
    "teeter dance": ([], "Chaotic", None, None),
    "thrash": (["Charge"], "Chaotic", "Spear", None),
    "struggle": (["Blunt"], "Chaotic", "Gauntlet", None),
    "rage": ([], "Aggressive", None, None),
    "shell smash": (["Fast"], "Aggressive", None, None),
    "quiver dance": (["Fast"], "Aggressive", None, None),
    "coil": (["Constrict"], "Aggressive", "Whip", None),
    "acid armor": (["Guard"], None, "Shield", None),
    "defense curl": (["Guard"], None, "Shield", None),
    "cotton guard": (["Guard"], None, "Shield", None),
    "aqua jet": (["Fast", "Charge"], None, "Spear", None),
    "ice shard": (["Fast", "Shoots"], None, "Staff", None),
    "bullet punch": (["Fast", "Blunt"], None, "Gauntlet", None),
    "mach punch": (["Fast", "Blunt"], None, "Gauntlet", None),
    "vacuum wave": (["Fast", "Repel"], None, "Staff", None),
    "accelerock": (["Fast", "Charge"], None, "Spear", None),
    "jet punch": (["Fast", "Blunt"], None, "Gauntlet", None),
    "icicle crash": (["DropFromSky", "Pierce"], None, "Spear", None),
    "icicle spear": (["Shoots", "Pierce"], None, "Spear", None),
    "bullet seed": (["Shoots", "Fast"], None, "Gun", None),
    "rock blast": (["Shoots", "Heavy"], None, "Gun", None),
    "seed bomb": (["Explodes"], None, "Bomb", None),
    "egg bomb": (["Explodes"], None, "Bomb", None),
    "mud bomb": (["Explodes"], None, "Bomb", None),
    "sludge wave": (["Repel", "Charge"], None, "Wall", None),
    "muddy water": (["Repel", "Charge"], None, "Wall", None),
    "night shade": (["Beam"], None, "Staff", None),
    "dream eater": (["Pull"], None, "Staff", None),
    "leech life": (["Pull", "Grab"], None, "Gauntlet", None),
    "drain punch": (["Pull", "Blunt"], None, "Gauntlet", None),
    "sonic boom": (["Shoots", "Fast"], None, "Staff", None),
    "tri attack": (["Shoots"], None, "Staff", None),
    "earth power": (["Launch"], None, "Pillar", None),
    "mud shot": (["Shoots"], None, "Staff", None),
    "sand tomb": (["Trap", "Spin"], None, "Cage", None),
    "rock tomb": (["Trap", "DropFromSky"], None, "Cage", None),
    "ancient power": (["Shoots", "Heavy"], None, "Boulder", None),
    "power gem": (["Beam"], None, "Staff", None),
    "future sight": (["DropFromSky"], None, "Ball", None),
    "doom desire": (["DropFromSky", "Explodes"], None, "Ball", None),
    "silver wind": (["Repel"], None, "Staff", None),
    "bug buzz": (["Repel"], None, "Staff", None),
    "air slash": (["Blade", "Shoots"], None, "Disc", None),
    "air cutter": (["Blade", "Shoots"], None, "Disc", None),
    "peck": (["Pierce"], None, "Spear", None),
    "drill peck": (["Pierce", "Spin"], None, "Spear", None),
    "poison sting": (["Pierce"], None, "Spear", None),
    "poison jab": (["Pierce"], None, "Spear", None),
    "horn attack": (["Pierce", "Charge"], None, "Spear", None),
    "fury attack": (["Pierce", "Fast"], None, "Spear", None),
    "twineedle": (["Pierce", "Fast"], None, "Spear", None),
    "spike cannon": (["Shoots", "Pierce"], None, "Cannon", None),
    "fire fang": (["Grab"], None, "Gauntlet", None),
    "ice fang": (["Grab"], None, "Gauntlet", None),
    "thunder fang": (["Grab"], None, "Gauntlet", None),
    "psychic fangs": (["Grab"], None, "Gauntlet", None),
    "jaw lock": (["Grab", "Trap"], None, "Gauntlet", None),
    "super fang": (["Grab"], None, "Gauntlet", None),
    "fire punch": (["Blunt"], None, "Gauntlet", None),
    "ice punch": (["Blunt"], None, "Gauntlet", None),
    "dynamic punch": (["Blunt", "Heavy"], "Chaotic", "Gauntlet", None),
    "focus punch": (["Blunt", "Heavy"], None, "Gauntlet", None),
    "sky uppercut": (["Blunt", "Launch"], None, "Gauntlet", None),
    "karate chop": (["Blade", "Blunt"], None, "Gauntlet", None),
    "cross chop": (["Blade", "Blunt"], None, "Gauntlet", None),
    "brick break": (["Blunt"], None, "Gauntlet", None),
    "double kick": (["Blunt", "Fast"], None, "Gauntlet", None),
    "rolling kick": (["Blunt", "Spin"], None, "Gauntlet", None),
    "jump kick": (["Blunt", "Launch"], None, "Gauntlet", None),
    "blaze kick": (["Blunt"], None, "Gauntlet", None),
    "triple kick": (["Blunt", "Fast"], None, "Gauntlet", None),
    "low kick": (["Blunt"], None, "Gauntlet", None),
    "comet punch": (["Blunt", "Fast"], None, "Gauntlet", None),
    "double slap": (["Blunt", "Fast"], None, "Gauntlet", None),
    "pound": (["Blunt"], None, "Gauntlet", None),
    "scratch": (["Blade"], None, "Gauntlet", None),
    "vise grip": (["Grab"], None, "Gauntlet", None),
    "crabhammer": (["Blunt", "Grab"], None, "Hammer", None),
    "clamp": (["Grab", "Trap"], None, "Gauntlet", None),
    "constrict": (["Constrict"], None, "Whip", None),
    "acid": (["Shoots"], None, "Staff", None),
    "acid spray": (["Shoots", "Sticky"], None, "Staff", None),
    "smog": (["Breath"], None, "Staff", None),
    "poison gas": (["Breath"], None, "Staff", None),
    "sludge": (["Shoots", "Sticky"], None, "Staff", None),
    "haze": (["Breath"], None, "Staff", "Shadow"),
    "mist": (["Breath"], None, "Staff", None),
    "powder snow": (["Breath"], None, "Staff", None),
    "icy wind": (["Breath", "Repel"], None, "Staff", None),
    "frost breath": (["Breath"], None, "Staff", None),
    "sheer cold": (["Breath", "Heavy"], None, "Staff", None),
    "freeze dry": (["Beam"], None, "Staff", None),
    "bubble": (["Shoots"], None, "Staff", None),
    "octazooka": (["Shoots", "Heavy"], None, "Cannon", None),
    "water spout": (["Beam", "Launch"], None, "Cannon", None),
    "aqua tail": (["Blunt"], None, "Whip", None),
    "poison tail": (["Blunt"], None, "Whip", None),
    "crush claw": (["Blade", "Grab"], None, "Gauntlet", None),
    "shadow punch": (["Blunt", "Homing"], None, "Gauntlet", None),
    "magical leaf": (["Shoots", "Homing"], None, "Staff", None),
    "shock wave": (["Shoots", "Homing"], None, "Staff", None),
    "feint attack": (["Blade", "Homing"], "Sneaky", "Sword", None),
    "zen headbutt": (["Charge", "Blunt"], None, "Spear", None),
    "head smash": (["Charge", "Heavy"], None, "Spear", None),
    "iron head": (["Charge", "Heavy"], None, "Spear", None),
    "steel wing": (["Blade", "Guard"], None, "Wings", None),
    "steamroller": (["Spin", "Heavy"], None, "Boulder", None),
    "rock polish": (["Fast"], None, None, None),
    "autotomize": (["Fast"], None, None, None),
    "tailwind": (["Fast"], None, None, "Wind"),
    "defog": (["Repel"], None, "Staff", "Wind"),
    "psych up": ([], "Aggressive", None, None),
    "hone claws": ([], "Aggressive", None, None),
    "work up": ([], "Aggressive", None, None),
    "shift gear": (["Fast"], "Aggressive", None, None),
    "curse": ([], "Chaotic", None, "Shadow"),
    "perish song": (["Repel"], "Chaotic", "Staff", None),
    "destiny bond": (["Link"], None, None, "Shadow"),
    "grudge": (["Link"], None, None, "Shadow"),
    "baton pass": (["Returning"], None, "Disc", None),
    "trick": ([], "Chaotic", None, None),
    "mimic": ([], "Chaotic", None, None),
    "copycat": ([], "Chaotic", None, None),
    "sketch": ([], "Chaotic", None, None),
}

# name keyword -> (mechanics, form). First match wins; order matters (specific before generic).
KEYWORDS = [
    (r"\b(beam|ray|laser)\b", ["Beam"], "Staff"),
    (r"\b(pump|cannon)\b", ["Beam"], "Cannon"),
    (r"\b(breath|thrower|blaze|flame|fire spin|inferno)\b", ["Breath"], "Staff"),
    (r"\b(explosion|destruct|self destruct|blown)\b", ["Explodes"], "Bomb"),
    (r"\b(bomb|grenade|mine)\b", ["Explodes"], "Bomb"),
    (r"\b(quake|tremor|magnitude|stomp)\b", ["Stomp"], "Hammer"),
    (r"\b(meteor|comet|falling|avalanche|slide)\b", ["DropFromSky"], "Boulder"),
    (r"\b(punch|fist|uppercut|jab|knuckle|chop)\b", ["Blunt"], "Gauntlet"),
    (r"\b(kick|stomp)\b", ["Blunt"], "Gauntlet"),
    (r"\b(hammer|club|mallet)\b", ["Blunt"], "Hammer"),
    (r"\b(claw|swipes|scratch|talon)\b", ["Blade"], "Gauntlet"),
    (r"\b(fang|fangs|bite|jaw|crunch)\b", ["Grab"], "Gauntlet"),
    (r"\b(blade|blades|sword|edge|slash|cut|cutter|scissor|axe|guillotine|razor)\b", ["Blade"], "Sword"),
    (r"\b(whip|lash|vine|vines|tendril)\b", ["Pull"], "Whip"),
    (r"\b(wrap|bind|squeeze|constrict|coil|wring)\b", ["Constrict"], "Whip"),
    (r"\b(tail)\b", ["Blunt"], "Whip"),
    (r"\b(horn|drill|spear|lance|needle|needles|sting|spike|spikes|peck|pin|arrow|arrows|dart)\b", ["Pierce"], "Spear"),
    (r"\b(wing|wings)\b", ["Blade"], "Wings"),
    (r"\b(spin|spinning|wheel|roll|rollout|gyro|vortex|tornado|twister|cyclone)\b", ["Spin"], "Disc"),
    (r"\b(missile|missiles|homing|seeker)\b", ["Shoots", "Homing"], "Gun"),
    (r"\b(ball|sphere|orb|bullet|shot|gun|seed|seeds|shard|shards|spit|blast)\b", ["Shoots"], "Staff"),
    (r"\b(wave|pulse|voice|shout|howl|roar|screech|song|burst|boom)\b", ["Repel"], "Staff"),
    (r"\b(wind|gust|storm|hurricane)\b", ["Repel"], "Staff"),
    (r"\b(tackle|charge|rush|ram|dash|crash|impact|headbutt|strike|blitz|smash|slam)\b", ["Charge"], "Spear"),
    (r"\b(shield|guard|wall|screen|armor|armour|barrier|protect|bunker)\b", ["Guard"], "Shield"),
    (r"\b(trap|tomb|prison|cage|web|net)\b", ["Trap"], "Cage"),
    (r"\b(drain|absorb|leech|siphon)\b", ["Pull"], "Whip"),
    (r"\b(throw|toss|grab|grip|seize|hold|clamp)\b", ["Grab"], "Gauntlet"),
    (r"\b(dance)\b", [], None),
]

# construct form for a mechanic set (first match wins) — used when neither table nor keyword gave a form
FORM_FOR = [
    ("Beam", "Staff"), ("Breath", "Staff"), ("Explodes", "Bomb"), ("Stomp", "Hammer"), ("DropFromSky", "Boulder"),
    ("Constrict", "Whip"), ("Pull", "Whip"), ("Grab", "Gauntlet"), ("Blade", "Sword"), ("Pierce", "Spear"),
    ("Charge", "Spear"), ("Flies", "Wings"), ("Spin", "Disc"), ("Trap", "Cage"), ("Guard", "Shield"),
    ("Barrier", "Wall"), ("Portal", "Wall"), ("Launch", "Pillar"), ("Bounce", "Trampoline"), ("Sticky", "Ball"),
    ("Attract", "Ball"), ("Repel", "Staff"), ("Shoots", "Staff"), ("Homing", "Ball"), ("Grow", "Staff"),
    ("Shrink", "Staff"), ("Returning", "Disc"), ("Blunt", "Gauntlet"), ("Heavy", "Hammer"), ("Fast", "Spear"),
]

# PokeAPI move_meta_ailments ids
AIL_SLEEP, AIL_CONFUSE, AIL_INFATUATE, AIL_TRAP, AIL_LEECH, AIL_YAWN = 2, 6, 7, 8, 18, 14


def build(norm, csv_dir: Path, form_defaults: dict):
    rd = lambda n: list(csv.DictReader(open(csv_dir / f"{n}.csv", encoding="utf-8")))  # noqa: E731
    types = {r["id"]: r["identifier"] for r in rd("types")}
    classes = {r["id"]: r["identifier"] for r in rd("move_damage_classes")}
    names = {r["move_id"]: r["name"] for r in rd("move_names") if r["local_language_id"] == "9"}
    meta = {r["move_id"]: r for r in rd("move_meta")}
    out = []
    seen = set()
    for m in rd("moves"):
        mid = m["id"]
        if int(mid) >= 10000:           # shadow / z-move variants
            continue
        display = names.get(mid) or m["identifier"].replace("-", " ").title()
        k = norm(display)
        if not k or k in seen or not re.fullmatch(r"[a-z0-9]+( [a-z0-9]+){0,5}", k):
            continue
        seen.add(k)
        tname = types.get(m["type_id"], "normal")
        el = TYPE_ELEMENT.get(tname)
        dclass = classes.get(m["damage_class_id"], "status")
        md = meta.get(mid, {})
        mech, temper, form = [], None, None
        famous = k in FAMOUS
        if famous:
            fm, temper, form, el_over = FAMOUS[k]
            mech = list(fm)
            if el_over:
                el = el_over
        else:
            for pat, ms, fo in KEYWORDS:
                if re.search(pat, k):
                    mech = list(ms)
                    form = fo
                    break
            ail = int(md.get("meta_ailment_id") or 0)
            cat = int(md.get("meta_category_id") or 0)
            drain = int(md.get("drain") or 0)
            prio = int(m.get("priority") or 0)
            max_hits = int(md.get("max_hits") or 0)
            if ail == AIL_TRAP and "Trap" not in mech and "Constrict" not in mech:
                mech.append("Trap")
            if ail == AIL_LEECH:
                mech.append("Sticky")
            if ail in (AIL_SLEEP, AIL_YAWN):
                temper = "Lazy"
            elif ail == AIL_CONFUSE:
                temper = "Chaotic"
            elif ail == AIL_INFATUATE:
                mech.append("Attract")
            if cat == 12:                              # force switch: blows the target away
                mech.append("Repel")
            if drain > 0 and "Pull" not in mech:
                mech.append("Pull")
            if drain < 0 and dclass == "physical" and "Charge" not in mech:
                mech.append("Charge")                  # recoil moves are reckless rams
            if prio > 0 and dclass != "status":
                mech.append("Fast")
            if max_hits >= 2 and dclass == "physical" and "Fast" not in mech:
                mech.append("Fast")
            if max_hits >= 2 and dclass == "special" and "Shoots" not in mech:
                mech.append("Shoots")
            if cat == 2 and not mech and temper is None:   # stat-boosting self moves
                temper = "Aggressive"
            if not mech and temper is None:
                if dclass == "physical":
                    mech = ["Blunt"]
                elif dclass == "special":
                    mech = ["Shoots"]
                else:
                    continue                            # status move with nothing to say: skip
        mech = sorted(set(mech))
        # self-buffs / movement moves don't recolour what they are grafted onto ("charizard use fly" stays Fire)
        el_override = famous and FAMOUS[k][3] is not None
        if not el_override and (dclass == "status" or set(mech) <= {"Flies", "Fast", "Guard", "Barrier", "Portal", "Shrink", "Grow", "Swarm", "Returning"}):
            el = None
        if form is None and mech:
            form = next((f for mm, f in FORM_FOR if mm in mech), "Staff")
        if form is None:
            form = "Blob0"                              # temperament/element-only: a glowing wisp of it
        # the construct must not carry mechanics the move doesn't (the sim grafts recipe mechanics)
        scale, mass = form_defaults.get(form, (1.0, 5.0))[:2]
        power = int(m.get("power") or 0)
        if power >= 120 and form not in ("Blob0",):
            scale *= 1.3
        out.append(dict(k=k, d=display, f=form, m=mech, e=el, t=temper, s=round(float(scale), 3),
                        w=round(float(mass), 3), famous=famous, cls=dclass, ty=tname))
    return out
