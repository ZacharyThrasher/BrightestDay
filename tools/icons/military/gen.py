"""Generates assets/icons/military.ron: broad, well-known military vocabulary (weapons, vehicles, fortifications,
support calls and radio yells) mapped onto the closed bd_core mechanics: delivery from the sky, volleys,
turrets, walls, summons. Usage: python tools/icons/military/gen.py [out.ron]

Only generic, public, non-classified terms. Names another pack already owns are dropped so a regeneration
stays warning-free (see tools/lexicon_build/dedupe_icon_names.py)."""
import glob
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
sys.path.insert(0, str(ROOT / "tools/icons/internet"))
sys.path.insert(0, str(ROOT / "tools/lexicon_build"))
from lib import E, R, ENTRIES, S, emit, check  # noqa: E402
from dedupe_icon_names import norm  # noqa: E402

PACK = "military.ron"
MIL = "Military (generic term)"
RADIO = "Military radio / call-out"

OLV = (0.4, 0.45, 0.25)
TAN = (0.75, 0.65, 0.45)
STL = (0.55, 0.58, 0.62)
FIRE = (1.0, 0.5, 0.1)


# ------------------------------------------------------------------------------------------ shapes
def sandbag_wall():
    """Three staggered courses of fat sandbags (4 m long, ~1.3 m high)."""
    s = S()
    y = 0.9
    for row in range(3):
        off = 0.0 if row % 2 == 0 else 1.6
        n = 5 if row % 2 == 0 else 4
        for i in range(n):
            x = -6.4 + off + i * 3.2
            s.ell((x, y, 0), (3.6, 1.9, 2.6), "P" if (i + row) % 2 == 0 else "A")
        y += 1.6
    return s


# ------------------------------------------------------------------------------------------ support calls
BOMB = R("bomb", "Bomb", ["Explodes"], "Fire", 0.6, 20)
E(["carpet bombing", "carpet bomb", "carpet bombs", "carpet bomb run", "carpet bombing run"], MIL,
  R("Carpet Bombing", "Bomb", ["DropFromSky", "Explodes"], "Fire", 0.7, 25, count=12), None,
  "a line of bombs walks across the target")
E(["bombing run", "air raid", "bombs away", "bombs away!"], MIL,
  R("Bombing Run", "Bomb", ["DropFromSky", "Explodes"], "Fire", 0.7, 25, count=8))
E(["air strike", "air strikes", "airstrikes", "call in an airstrike", "call in air support", "air support",
   "close air support", "calling in air support", "request air support"], MIL,
  R("Air Strike", "Bomb", ["DropFromSky", "Explodes"], "Fire", 0.7, 25, count=6))
E(["drone strike", "drone strikes", "predator strike", "hellfire missile"], MIL,
  R("Drone Strike", "Rocket", ["DropFromSky", "Explodes", "Homing"], "Fire", 1.2, 45, count=2))
E(["artillery barrage", "artillery strike", "artillery fire", "artillery bombardment", "barrage of artillery",
   "fire for effect", "fire mission", "shelling", "mortar strike", "mortar barrage", "mortar fire", "mortar rounds",
   "incoming artillery"], MIL,
  R("Artillery Barrage", "Bomb", ["DropFromSky", "Explodes", "Heavy"], "Fire", 0.8, 40, count=8))
E(["danger close", "danger close!", "broken arrow", "shock and awe"], RADIO,
  R("Danger Close", "Bomb", ["DropFromSky", "Explodes", "Heavy"], "Fire", 1.2, 80, count=12, temp="Aggressive", inten=1.0),
  None, "everything, everywhere, right on top of us")
E(["precision strike", "surgical strike", "smart bomb", "guided bomb", "laser guided bomb"], MIL,
  R("Precision Strike", "Rocket", ["DropFromSky", "Explodes", "Homing"], "Fire", 1.5, 60))
E(["bunker buster", "bunker busters"], MIL,
  R("Bunker Buster", "Bomb", ["DropFromSky", "Explodes", "Pierce", "Heavy"], "Fire", 2.0, 900))
E(["mother of all bombs"], MIL,
  R("Mother of All Bombs", "Bomb", ["DropFromSky", "Explodes", "Heavy"], "Fire", 4.0, 5000))
E(["cluster bomb", "cluster bombs", "cluster munition"], MIL,
  R("Cluster Bomb", "Bomb", ["DropFromSky", "Explodes", "Swarm"], "Fire", 0.5, 10, count=8))
E(["napalm", "napalm strike", "napalm bomb", "napalm run", "drop napalm"], MIL,
  R("Napalm", "Bomb", ["DropFromSky", "Explodes", "Sticky"], "Fire", 0.8, 30, count=6, accent=FIRE))
E(["rods from god", "rod from god", "kinetic bombardment", "orbital kinetic strike"], MIL,
  R("Rods from God", "Spear", ["DropFromSky", "Pierce", "Heavy"], "Light", 4.0, 3000, count=3, accent=STL))
E(["incoming", "incoming!", "incoming fire"], RADIO,
  R("Incoming", "Bomb", ["DropFromSky", "Explodes"], "Fire", 0.7, 25, count=3))
E(["fox two", "fox 2", "fox three", "fox 3", "fox one", "fox 1", "missile away", "missiles away"], RADIO,
  R("Fox Two", "Rocket", ["Homing", "Fast", "Explodes", "Thrust"], "Fire", 1.6, 60))
E(["supply crate", "ammo drop", "airdrop"], MIL,
  R("Supply Drop", "Anvil", ["DropFromSky", "Heavy"], "Light", 1.5, 200, accent=OLV))
E(["paratroopers", "paratrooper", "paratroops", "airborne troops", "send in the paratroopers"], MIL,
  R("Paratroopers", "Biped", ["SlowFall", "DropFromSky", "Shoots", "Swarm"], "Light", 1.8, 80, count=4,
    accent=OLV, temp="Aggressive", inten=0.7))
E(["reinforcements", "send reinforcements", "fire team", "fireteam", "squad of soldiers", "army of soldiers",
   "platoon"], MIL,
  R("Reinforcements", "Biped", ["Shoots", "Swarm", "Walks"], "Light", 1.8, 80, count=4, accent=OLV,
    temp="Protective", inten=0.7))
E(["suppressing fire", "suppressive fire", "covering fire", "cover fire", "open fire", "fire at will",
   "lay down fire"], RADIO,
  R("Suppressing Fire", "Gun", ["Shoots", "Fast"], "Light", 1.2, 12, feat=["Barrels"]))
E(["fire in the hole", "fire in the hole!", "frag out", "frag out!", "grenade out", "grenade!"], RADIO,
  R("Frag Out", "Bomb", ["Explodes"], "Fire", 0.5, 1, accent=OLV))
E(["smoke screen", "smokescreen", "pop smoke", "smoke wall"], MIL,
  R("Smoke Screen", "Wall", ["Barrier"], "Shadow", 5.0, 5))
E(["hold the line", "take cover", "dig in", "fortify"], RADIO,
  R("Hold the Line", "Wall", ["Barrier", "Heavy"], "Earth", 3.0, 800, accent=TAN), sandbag_wall())

# ------------------------------------------------------------------------------------------ weapons
E(["rpg", "rpg-7", "rpg 7", "rocket propelled grenade", "rocket-propelled grenade"], MIL,
  R("RPG", "Cannon", ["Shoots", "Explodes"], "Fire", 1.2, 8, accent=OLV,
    payload=R("rocket", "Rocket", ["Explodes", "Thrust"], "Fire", 0.6, 3)))
E(["icbm", "intercontinental ballistic missile", "ballistic missile", "nuclear missile", "nuke missile"], MIL,
  R("ICBM", "Rocket", ["DropFromSky", "Explodes", "Heavy", "Homing"], "Atomic", 6.0, 8000))
E(["cruise missile", "cruise missiles", "tomahawk missile", "tomahawk cruise missile"], MIL,
  R("Cruise Missile", "Rocket", ["Explodes", "Homing", "Thrust", "Fast"], "Fire", 3.0, 800))
E(["emp", "emp blast", "emp bomb", "emp grenade", "emp pulse", "electromagnetic pulse"], MIL,
  R("EMP", "Bomb", ["Explodes", "Repel"], "Electric", 1.2, 5, accent=(0.4, 0.7, 1.0)))
E(["molotov", "molotov cocktail", "molotovs", "fire bottle"], MIL,
  R("Molotov Cocktail", "Bomb", ["Explodes", "Sticky"], "Fire", 0.4, 1, accent=FIRE))
E(["landmine", "land mine", "landmines", "land mines", "anti tank mine", "proximity mine"], MIL,
  R("Landmine", "Bomb", ["Explodes", "Trap"], "Fire", 0.5, 8, accent=OLV))
E(["claymore mine", "claymore mines", "directional mine"], MIL,
  R("Claymore Mine", "Bomb", ["Explodes", "Trap", "Shoots"], "Fire", 0.4, 2, accent=OLV))
E(["smoke grenade", "smoke grenades", "smoke canister"], MIL,
  R("Smoke Grenade", "Bomb", ["Explodes", "Repel"], "Shadow", 0.4, 1))
E(["frag grenade", "fragmentation grenade", "frag", "frags"], MIL,
  R("Frag Grenade", "Bomb", ["Explodes"], "Fire", 0.4, 1, accent=OLV))
E(["flashbang", "flash bang", "flashbang grenade", "stun grenade", "flash grenade"], MIL,
  R("Flashbang", "Bomb", ["Explodes", "Repel"], "Light", 0.4, 1, accent=(1.0, 1.0, 0.9)))
E(["semtex", "satchel charge", "breaching charge", "demo charge", "sticky c4"], MIL,
  R("Satchel Charge", "Bomb", ["Explodes", "Sticky"], "Fire", 0.5, 3, accent=OLV))
E(["flak cannon", "flak", "anti aircraft gun", "aa gun", "anti-aircraft gun", "triple a"], MIL,
  R("Flak Cannon", "Cannon", ["Shoots", "Homing", "Explodes"], "Fire", 2.0, 500, feat=["Turret", "Barrels"]))
E(["sentry gun", "machine gun turret", "gun turret", "auto turret", "autoturret"], MIL,
  R("Sentry Gun", "Pillar", ["Shoots", "Homing"], "Light", 1.4, 80, feat=["Turret", "Barrels"]))
E(["kamikaze drone", "suicide drone", "attack drone", "fpv drone", "loitering munition"], MIL,
  R("Attack Drone", "Rocket", ["Homing", "Explodes", "Flies"], "Fire", 1.0, 20, feat=["Propeller"]))
E(["drone swarm", "swarm of drones", "drone army"], MIL,
  R("Drone Swarm", "Bird", ["Flies", "Shoots", "Swarm", "Homing"], "Light", 0.5, 3, count=8, feat=["Rotor"]))
E(["tank shell", "tank round", "sabot round", "artillery shell", "howitzer shell", "mortar round", "mortar shell"], MIL,
  R("Shell", "Rocket", ["Explodes", "Fast", "Heavy"], "Fire", 0.8, 20))
E(["sniper nest", "sniper team", "sniper squad"], MIL,
  R("Sniper Team", "Biped", ["Shoots", "Pierce"], "Light", 1.8, 80, count=2, accent=OLV, temp="Sneaky", inten=0.8))

# ------------------------------------------------------------------------------------------ vehicles
E(["apc", "armored personnel carrier", "armoured personnel carrier", "ifv", "infantry fighting vehicle",
   "bradley", "bradley ifv", "stryker"], MIL,
  R("APC", "Vehicle", ["Charge", "Heavy", "Shoots"], "Light", 6.0, 14000, accent=OLV,
    feat=["Wheels", "Turret", "Armor", "Windows"]))
E(["abrams", "abrams tank", "m1 abrams", "sherman tank", "tiger tank", "panzer", "main battle tank", "battle tank",
   "t-90", "leopard tank", "challenger tank"], MIL,
  R("Battle Tank", "Vehicle", ["Shoots", "Heavy", "Charge", "Piloted"], "Light", 7.0, 60000, accent=OLV,
    feat=["Treads", "Turret", "Barrels", "Armor"]))
E(["b-52", "b52", "b 52", "b-52 bomber", "b-2", "b-2 spirit", "b2 bomber", "b-2 bomber", "stratofortress",
   "heavy bomber", "strategic bomber"], MIL,
  R("Heavy Bomber", "Vehicle", ["Flies", "Fast", "Shoots"], "Light", 16.0, 80000, accent=STL, feat=["Jet", "Cockpit"],
    payload=BOMB))
E(["ac-130", "ac 130", "ac130", "ac-130 gunship", "spectre gunship", "flying gunship"], MIL,
  R("AC-130 Gunship", "Vehicle", ["Flies", "Shoots", "DropFromSky"], "Fire", 16.0, 70000, accent=STL,
    feat=["Propeller", "Barrels", "Cockpit"]))
E(["a-10", "a10", "a-10 warthog", "warthog jet", "brrrt", "brrrrt", "brrt"], MIL,
  R("A-10 Warthog", "Vehicle", ["Flies", "Shoots", "Fast"], "Light", 8.0, 20000, accent=STL,
    feat=["Jet", "Barrels", "Cockpit"]))
E(["f-16", "f16", "f-22", "f22", "f-35", "f35", "f-15", "f15", "f-18", "f18", "harrier jet", "stealth fighter",
   "fighter plane", "fighter aircraft", "jet fighter", "spitfire plane"], MIL,
  R("Fighter Jet", "Vehicle", ["Flies", "Fast", "Shoots", "Thrust"], "Light", 7.0, 15000, accent=STL,
    feat=["Jet", "Cockpit"], payload=R("missile", "Rocket", ["Homing", "Explodes"], "Fire", 0.8, 10)))
E(["chinook helicopter", "chinook chopper", "ch-47", "transport helicopter", "huey", "huey helicopter",
   "little bird helicopter"], MIL,
  R("Transport Helicopter", "Vehicle", ["Flies", "Charge"], "Light", 8.0, 10000, accent=OLV,
    feat=["Rotor", "Cockpit", "Windows"]))
E(["gunship helicopter", "cobra helicopter", "hind helicopter", "attack chopper", "attack choppers"], MIL,
  R("Attack Helicopter", "Vehicle", ["Flies", "Shoots", "Homing"], "Light", 6.0, 8000, accent=OLV,
    feat=["Rotor", "Cockpit", "Barrels"]))
E(["mq-9 reaper", "reaper drone", "global hawk", "surveillance drone", "spy drone", "recon drone",
   "call in a uav", "call in uav", "uav online", "uav"], MIL,
  R("UAV", "Vehicle", ["Flies", "Homing", "Shoots"], "Light", 4.0, 2000, accent=STL, feat=["Propeller", "Antenna"]))
E(["warship", "war ship", "navy ship", "frigate", "cruiser warship", "dreadnought battleship"], MIL,
  R("Warship", "Vehicle", ["Shoots", "Heavy"], "Water", 30.0, 900000, accent=STL,
    feat=["Turret", "Barrels", "Mast", "Antenna"]))
E(["torpedo", "torpedoes", "torpedo strike"], MIL,
  R("Torpedo", "Rocket", ["Homing", "Explodes", "Fast"], "Water", 2.0, 300))

# ------------------------------------------------------------------------------------------ fortifications
E(["sandbags", "sandbag", "sandbag wall", "wall of sandbags", "sand bags", "sand bag wall", "sandbag barricade"], MIL,
  R("Sandbag Wall", "Wall", ["Barrier", "Heavy"], "Earth", 3.0, 800, accent=TAN), sandbag_wall(),
  "stacked sandbags: low, heavy cover")
E(["bunker", "concrete bunker", "military bunker", "underground bunker"], MIL,
  R("Bunker", "Dome", ["Barrier", "Heavy"], "Earth", 4.0, 20000, accent=STL, feat=["Windows"]))
E(["pillbox", "pill box", "gun bunker", "machine gun nest", "mg nest"], MIL,
  R("Pillbox", "Dome", ["Barrier", "Shoots", "Heavy"], "Earth", 3.0, 15000, accent=STL, feat=["Turret", "Barrels"]))
E(["trench", "trenches", "trench line", "foxhole", "fox hole", "dugout", "earthworks", "berm"], MIL,
  R("Trench", "Wall", ["Barrier"], "Earth", 2.0, 2000, accent=TAN))
E(["barbed wire", "barbed wire fence", "razor wire", "concertina wire", "razorwire", "barbwire"], MIL,
  R("Barbed Wire", "Wall", ["Barrier", "Blade", "Sticky"], "Light", 2.0, 30, accent=STL, feat=["Spikes"]))
E(["tank trap", "tank traps", "czech hedgehog", "dragon's teeth", "dragons teeth"], MIL,
  R("Tank Trap", "Wall", ["Barrier", "Pierce", "Heavy"], "Earth", 2.0, 1500, accent=STL, feat=["Spikes"]))
E(["guard tower", "sentry tower", "military watchtower"], MIL,
  R("Guard Tower", "Pillar", ["Barrier", "Shoots"], "Light", 7.0, 3000, feat=["Windows", "Headlight"]))
E(["checkpoint", "road block", "roadblock", "jersey barrier"], MIL,
  R("Roadblock", "Wall", ["Barrier", "Heavy"], "Earth", 2.0, 2000, accent=STL))
E(["hesco barrier", "hesco", "blast barrier"], MIL,
  R("Blast Barrier", "Wall", ["Barrier", "Heavy"], "Earth", 3.0, 5000, accent=TAN))

# ------------------------------------------------------------------------------------------ emit
OTHER = set()
for p in glob.glob(str(ROOT / "assets/icons.ron")) + glob.glob(str(ROOT / "assets/icons/*.ron")):
    if p.replace("\\", "/").endswith("/" + PACK):
        continue
    src = open(p, encoding="utf-8").read()
    for m in re.finditer(r"names:\s*\[(.*?)\]", src, re.S):
        OTHER.update(norm(x) for x in re.findall(r'"([^"]*)"', m.group(1)))
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
    if shape is not None:
        check(shape, names[0])
out, _ = emit()
hdr = ("// Brightest Day icon pack: military vocabulary (weapons, vehicles, fortifications, support calls, radio yells).\n"
       "// GENERATED by tools/icons/military/gen.py - edit that, then re-run it. Schema: docs/ICONS_SCHEMA.md.\n"
       "// Generic public terms only, mapped onto existing mechanics (sky delivery, volleys, turrets, walls, summons).\n")
out = hdr + out.split("\n", 3)[3]
dst = Path(sys.argv[1]) if len(sys.argv) > 1 else ROOT / "assets/icons" / PACK
dst.write_text(out, encoding="utf-8")
print(len(ENTRIES), "entries,", sum(len(e[0]) for e in ENTRIES), "names ->", dst)
