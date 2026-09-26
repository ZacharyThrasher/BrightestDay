import sys
from dsl import R, E, emit, emit_gallery, ENTRIES
from shapes import get

# ------------------------------------------------------------------ shared payloads
def fireball(el='Fire', mech=('Bounce',), scale=0.4):
    return R("fireball", "Ball", list(mech), el, scale, 2.0)

def rocket(name="rocket", el='Fire', homing=True, scale=0.8):
    return R(name, "Rocket", ["Explodes"] + (["Homing"] if homing else []), el, scale, 8.0)

def bolt(name, el='Light', mech=('Fast',), scale=0.3):
    return R(name, "Ball", list(mech), el, scale, 1.0)

RED = (0.9, 0.15, 0.15); GOLD = (1.0, 0.8, 0.2); BLUE = (0.25, 0.5, 1.0); CYAN = (0.3, 0.95, 0.95)
GREEN = (0.3, 0.9, 0.3); PINK = (1.0, 0.45, 0.7); ORANGE = (1.0, 0.55, 0.1); PURPLE = (0.7, 0.3, 1.0)
WHITE = (0.95, 0.95, 0.95); STEEL = (0.75, 0.78, 0.82); YELLOW = (1.0, 0.9, 0.2); BLACK = (0.12, 0.12, 0.14)

# ================================================================== THE LEGEND OF ZELDA
Z = "Nintendo (The Legend of Zelda)"
E(["master sword", "the master sword", "blade of evil's bane", "sword that seals the darkness", "links sword",
   "link's sword", "zelda sword", "mastersword"],
  R("Master Sword", "Sword", ["Blade", "Shoots"], "Light", 1.4, 3.0, accent=BLUE,
    payload=R("sword beam", "Ball", ["Fast", "Pierce"], "Light", 0.5, 1.0)), Z, get("master_sword"))
E(["hylian shield", "links shield", "link's shield", "zelda shield"],
  R("Hylian Shield", "Shield", ["Barrier", "Guard"], "Light", 1.2, 12.0, accent=RED), Z, get("hylian_shield"))
E(["hookshot", "longshot", "clawshot", "hook shot", "claw shot"],
  R("Hookshot", "Whip", ["Pierce", "Pull"], "Light", 1.0, 3.0, accent=STEEL), Z, get("hookshot"))
E(["guardian", "guardian stalker", "decayed guardian", "botw guardian", "breath of the wild guardian"],
  R("Guardian", "Arachnid", ["Beam", "Walks"], "Light", 3.0, 800.0, accent=ORANGE, temp="Aggressive", inten=0.7),
  Z, get("guardian"))
E(["cucco", "cuccos", "angry cucco", "cucco swarm", "cucco revenge", "zelda chicken"],
  R("Cucco", "Bird", ["Flies", "Homing", "Swarm"], "Light", 0.5, 1.5, 14, accent=WHITE, temp="Aggressive", inten=0.95), Z)
E(["bombchu", "bombchus"],
  R("Bombchu", "Bomb", ["Explodes", "Fast", "Homing", "Walks"], "Fire", 0.5, 2.0), Z)
E(["paraglider", "zelda paraglider", "links paraglider", "link's paraglider"],
  R("Paraglider", "Wings", ["Glide", "SlowFall"], "Wind", 2.0, 4.0), Z)
E(["deku leaf"], R("Deku Leaf", "Staff", ["Glide", "Repel"], "Wind", 1.2, 1.0, accent=GREEN), Z)
E(["ultrahand", "ultra hand", "zonai ultrahand"],
  R("Ultrahand", "Gauntlet", ["Grab", "Link"], "Light", 1.0, 5.0, accent=GREEN), Z)
E(["zonai rocket"], R("Zonai Rocket", "Rocket", ["Launch", "Thrust"], "Wind", 1.0, 5.0), Z)
E(["lynel", "silver lynel", "white-maned lynel"],
  R("Lynel", "Quadruped", ["Breath", "Charge", "Shoots"], "Fire", 3.2, 900.0, payload=R("lynel arrow", "Ball", ["Pierce", "Fast"], "Fire", 0.6, 1.0),
    temp="Aggressive", inten=0.8), Z)
E(["ganon", "calamity ganon", "ganondorf", "dark beast ganon", "demon king ganondorf"],
  R("Ganon", "Kaiju", ["ArticulatedTail", "Breath", "Charge", "Stomp"], "Shadow", 14.0, 20000.0, accent=(0.9, 0.3, 0.6),
    temp="Aggressive", inten=0.9), Z)
E(["octorok"], R("Octorok", "Blob", ["Shoots"], "Earth", 0.9, 30.0, payload=R("rock", "Boulder", [], "Earth", 0.4, 5.0),
                  temp="Fearful", inten=0.5), Z)

# ================================================================== SUPER MARIO / MARIO KART
M = "Nintendo (Super Mario)"
MK = "Nintendo (Mario Kart)"
E(["green shell", "koopa shell", "green koopa shell", "turtle shell", "mario shell", "shell"],
  R("Green Shell", "Ball", ["Blunt", "Fast", "Ricochet"], "Light", 0.8, 6.0, accent=GREEN), MK, get("green_shell"))
E(["red shell", "red koopa shell", "homing shell"],
  R("Red Shell", "Ball", ["Blunt", "Homing", "Ricochet"], "Light", 0.8, 6.0, accent=RED), MK, get("red_shell"))
E(["blue shell", "spiny shell", "spiny blue shell", "winged blue shell", "blue spiny shell"],
  R("Spiny Shell", "Bomb", ["DropFromSky", "Explodes", "Flies", "Homing"], "Electric", 0.9, 8.0, accent=BLUE,
    temp="Aggressive", inten=1.0), MK, get("spiny_shell"))
E(["bob-omb", "bob omb", "bobomb", "bob-ombs", "walking bomb"],
  R("Bob-omb", "Bomb", ["Explodes", "Walks"], "Fire", 1.0, 8.0, accent=GOLD, temp="Chaotic", inten=0.6), M, get("bob_omb"))
E(["bullet bill", "bullet bills", "bill blaster bullet"],
  R("Bullet Bill", "Rocket", ["Blunt", "Explodes", "Fast"], "Light", 1.4, 20.0, accent=WHITE), M, get("bullet_bill"))
E(["banzai bill", "king bill"],
  R("Banzai Bill", "Rocket", ["Explodes", "Heavy"], "Fire", 5.0, 400.0, accent=WHITE), M)
E(["bill blaster", "bullet bill cannon", "bullet bill launcher"],
  R("Bill Blaster", "Cannon", ["Shoots"], "Light", 1.4, 40.0, payload=R("Bullet Bill", "Rocket", ["Explodes", "Fast"], "Light", 1.0, 10.0)), M)
E(["pow block", "pow", "p.o.w. block", "pow box"],
  R("POW Block", "Anvil", ["DropFromSky", "Stomp"], "Earth", 1.2, 60.0, accent=BLUE), M, get("pow_block"))
E(["super mushroom", "mario mushroom", "red mushroom", "mushroom power-up", "mushroom powerup"],
  R("Super Mushroom", "Ball", ["Bounce", "Grow"], "Light", 0.7, 2.0, accent=WHITE), M, get("super_mushroom"))
E(["mega mushroom", "giant mushroom"],
  R("Mega Mushroom", "Ball", ["Grow", "Heavy"], "Light", 1.4, 20.0, accent=GOLD), M)
E(["mini mushroom", "tiny mushroom", "blue mushroom"],
  R("Mini Mushroom", "Ball", ["Bounce", "Shrink"], "Light", 0.5, 1.0, accent=BLUE), M)
E(["super star", "starman", "invincibility star", "power star", "mario star"],
  R("Super Star", "Ball", ["Bounce", "Fast", "Ricochet"], "Light", 1.0, 3.0, accent=YELLOW), M, get("super_star"))
E(["fire flower", "mario fireball", "fire mario"],
  R("Fire Flower", "Staff", ["Shoots"], "Fire", 0.9, 1.0, accent=ORANGE, payload=fireball()), M)
E(["ice flower", "ice mario"],
  R("Ice Flower", "Staff", ["Shoots"], "Ice", 0.9, 1.0, accent=CYAN, payload=fireball('Ice')), M)
E(["cape feather", "cape mario", "mario cape"],
  R("Cape Feather", "Cape", ["Glide", "SlowFall"], "Wind", 1.2, 1.0, accent=GOLD), M)
E(["bowser", "king koopa", "king bowser", "bowser koopa"],
  R("Bowser", "Biped", ["ArticulatedTail", "Breath", "Heavy", "Stomp"], "Fire", 4.0, 1200.0, accent=GOLD,
    temp="Aggressive", inten=0.85), M, get("bowser"))
E(["giga bowser", "dark bowser"],
  R("Giga Bowser", "Kaiju", ["ArticulatedTail", "Breath", "Stomp"], "Fire", 16.0, 30000.0, accent=GOLD,
    temp="Aggressive", inten=1.0), M)
E(["chain chomp", "chomp", "chain chomps"],
  R("Chain Chomp", "Blob", ["Blunt", "Charge", "Heavy"], "Light", 2.2, 900.0, accent=WHITE, feat=["Chains"],
    temp="Aggressive", inten=0.9), M, get("chain_chomp"))
E(["thwomp", "thwomps", "whomp"],
  R("Thwomp", "Anvil", ["DropFromSky", "Heavy", "Stomp"], "Earth", 2.8, 2000.0, accent=STEEL, feat=["Spikes"]), M, get("thwomp"))
E(["goomba", "goombas", "goomba swarm"],
  R("Goomba", "Biped", ["Swarm"], "Light", 0.6, 3.0, 8, temp="Aggressive", inten=0.4), M)
E(["piranha plant", "petey piranha"],
  R("Piranha Plant", "Serpent", ["Grab", "Shoots"], "Poison", 1.6, 20.0, accent=RED, payload=fireball(),
    temp="Lazy", inten=0.6), M)
E(["boo", "boos", "king boo", "mario ghost"],
  R("Boo", "Blob", ["Flies", "Homing"], "Shadow", 1.0, 1.0, 3, accent=WHITE, temp="Sneaky", inten=0.9), M)
E(["lakitu", "lakitu cloud"],
  R("Lakitu", "Platform", ["DropFromSky", "Flies", "Shoots"], "Light", 1.5, 10.0,
    payload=R("spiny egg", "Ball", ["Pierce", "Walks"], "Light", 0.5, 2.0, feat=["Spikes"]), temp="Fearful", inten=0.6), M)
E(["yoshi", "yoshis", "ride yoshi"],
  R("Yoshi", "Biped", ["Grab", "Piloted", "Pull"], "Light", 1.8, 60.0, accent=GREEN, temp="Playful", inten=0.6), M)
E(["hammer bro", "hammer bros", "hammer brothers"],
  R("Hammer Bro", "Biped", ["Shoots"], "Light", 1.2, 40.0, payload=R("hammer", "Hammer", ["Blunt"], "Light", 0.6, 3.0),
    temp="Playful", inten=0.6), M)
E(["mario kart", "kart", "go-kart", "go kart", "standard kart", "pipe frame"],
  R("Kart", "Vehicle", ["Fast", "Piloted"], "Light", 1.6, 150.0, feat=["Exhaust", "Wheels"]), MK)
E(["mario kart lightning", "lightning bolt item", "shock item"],
  R("Lightning", "Pillar", ["DropFromSky", "Shrink"], "Electric", 3.0, 5.0, accent=YELLOW), MK)
E(["banana peel", "mario kart banana", "banana item"],
  R("Banana Peel", "Platform", ["Launch", "Sticky"], "Light", 0.6, 1.0, accent=YELLOW), MK)
E(["golden mushroom"],
  R("Golden Mushroom", "Ball", ["Fast", "Grow"], "Light", 0.7, 2.0, accent=GOLD), MK)
E(["super bell", "cat suit", "cat mario"],
  R("Super Bell", "Gauntlet", ["Blade", "Fast"], "Light", 0.8, 2.0, accent=GOLD, feat=["Bell"]), M)

# ================================================================== DONKEY KONG
DK = "Nintendo (Donkey Kong)"
E(["dk barrel", "donkey kong barrel", "barrel", "rolling barrel", "wooden barrel"],
  R("DK Barrel", "Boulder", ["Blunt", "Heavy"], "Earth", 1.3, 80.0, accent=(0.85, 0.55, 0.25)), DK, get("dk_barrel"))
E(["donkey kong", "dk", "donkey kong bananza"],
  R("Donkey Kong", "Biped", ["Grab", "Stomp"], "Earth", 2.4, 400.0, temp="Aggressive", inten=0.6), DK)
E(["barrel cannon", "blast barrel", "dk barrel cannon"],
  R("Barrel Cannon", "Pillar", ["Launch"], "Light", 1.6, 60.0), DK)
E(["bananza punch", "dk punch", "ground pound punch"],
  R("Bananza Punch", "Gauntlet", ["Blunt", "Stomp"], "Earth", 1.0, 20.0), DK)

# ================================================================== KIRBY
K = "Nintendo (Kirby)"
E(["kirby", "pink puffball", "kirby inhale", "poyo"],
  R("Kirby", "Blob", ["Attract", "Flies", "Grab"], "Wind", 1.0, 6.0, accent=PINK, temp="Playful", inten=0.7), K, get("kirby"))
E(["warp star", "kirby star", "kirby's warp star"],
  R("Warp Star", "Vehicle", ["Fast", "Flies", "Piloted"], "Light", 2.0, 20.0, accent=YELLOW), K, get("warp_star"))
E(["king dedede hammer", "dedede hammer", "jet hammer", "king dedede"],
  R("Jet Hammer", "Hammer", ["Blunt", "Heavy", "Thrust"], "Fire", 2.0, 60.0, feat=["Jet"]), K)

# ================================================================== METROID / STAR FOX
MT = "Nintendo (Metroid)"
E(["arm cannon", "samus arm cannon", "power beam", "samus cannon", "charge beam"],
  R("Arm Cannon", "Cannon", ["Beam", "Shoots"], "Light", 1.2, 15.0, accent=ORANGE,
    payload=R("charge shot", "Ball", ["Fast", "Pierce"], "Light", 0.6, 2.0)), MT, get("arm_cannon"))
E(["metroid", "metroids", "baby metroid", "larva metroid"],
  R("Metroid", "Blob", ["Flies", "Grab", "Sticky"], "Electric", 1.0, 5.0, accent=GREEN, temp="Aggressive", inten=0.8), MT, get("metroid"))
E(["morph ball bomb", "morph ball", "power bomb"],
  R("Morph Ball Bomb", "Bomb", ["Explodes", "Launch"], "Light", 0.4, 1.0, 3), MT)
E(["grapple beam", "samus grapple"],
  R("Grapple Beam", "Whip", ["Pull"], "Electric", 1.0, 2.0), MT)
E(["ridley", "meta ridley"],
  R("Ridley", "Bird", ["ArticulatedTail", "Breath", "Flies", "Grab"], "Fire", 4.0, 1200.0, accent=PURPLE,
    temp="Aggressive", inten=0.9), MT)
SF = "Nintendo (Star Fox)"
E(["arwing", "arwings", "star fox ship", "fox mccloud ship"],
  R("Arwing", "Vehicle", ["Fast", "Flies", "Piloted", "Shoots"], "Light", 3.0, 800.0, accent=BLUE,
    payload=R("twin laser", "Ball", ["Fast"], "Electric", 0.4, 1.0), feat=["Cockpit", "Jet"]), SF, get("arwing"))
E(["landmaster", "landmaster tank"],
  R("Landmaster", "Vehicle", ["Piloted", "Shoots"], "Light", 3.0, 8000.0, feat=["Armor", "Treads", "Turret"]), SF)
E(["smart bomb", "nova bomb"],
  R("Smart Bomb", "Bomb", ["Explodes", "Homing"], "Atomic", 0.8, 5.0), SF)

# ================================================================== POKEMON (items; creatures come from PokeAPI)
PK = "Nintendo/Game Freak (Pokemon)"
E(["master ball", "masterball"],
  R("Master Ball", "Ball", ["Homing", "Trap"], "Psychic", 0.4, 1.0, accent=PURPLE), PK)
E(["rare candy"], R("Rare Candy", "Ball", ["Grow"], "Light", 0.3, 0.5, accent=BLUE), PK)
E(["pal sphere", "palworld sphere"], R("Pal Sphere", "Ball", ["Trap"], "Light", 0.4, 1.0, accent=BLUE), "Pocketpair (Palworld)")

# ================================================================== MINECRAFT
MC = "Mojang (Minecraft)"
E(["tnt", "minecraft tnt", "tnt block", "dynamite block"],
  R("TNT", "Bomb", ["Explodes", "Heavy"], "Fire", 1.0, 20.0, accent=RED), MC, get("tnt"))
E(["creeper", "creepers", "minecraft creeper", "aw man"],
  R("Creeper", "Quadruped", ["Explodes", "Walks"], "Fire", 1.7, 40.0, accent=(0.3, 0.8, 0.3), temp="Sneaky", inten=0.9),
  MC, get("creeper"))
E(["diamond sword", "minecraft sword", "minecraft diamond sword", "netherite sword"],
  R("Diamond Sword", "Sword", ["Blade"], "Light", 1.2, 3.0, accent=CYAN), MC, get("diamond_sword"))
E(["ender dragon", "enderdragon", "the ender dragon", "minecraft dragon"],
  R("Ender Dragon", "Bird", ["ArticulatedTail", "Breath", "Flies"], "Shadow", 4.0, 2000.0, accent=PURPLE,
    temp="Aggressive", inten=0.8), MC, get("ender_dragon"))
E(["diamond pickaxe", "pickaxe", "minecraft pickaxe", "netherite pickaxe"],
  R("Diamond Pickaxe", "Axe", ["Pierce"], "Earth", 1.0, 4.0, accent=CYAN), MC)
E(["minecraft anvil", "falling anvil minecraft"],
  R("Minecraft Anvil", "Anvil", ["DropFromSky", "Heavy"], "Light", 1.0, 500.0), MC)
E(["enderman", "endermen"],
  R("Enderman", "Biped", ["Grab", "Portal"], "Shadow", 2.9, 70.0, accent=PURPLE, temp="Sneaky", inten=0.7), MC)
E(["ender pearl", "enderpearl"],
  R("Ender Pearl", "Ball", ["Portal"], "Psychic", 0.3, 0.5, accent=(0.2, 0.7, 0.6)), MC)
E(["minecraft trident", "trident loyalty", "channeling trident"],
  R("Trident", "Spear", ["Pierce", "Returning"], "Electric", 1.6, 5.0, accent=CYAN), MC)
E(["elytra"], R("Elytra", "Wings", ["Glide", "Thrust"], "Wind", 1.8, 2.0, accent=PURPLE), MC)
E(["ghast", "ghasts"],
  R("Ghast", "Blob", ["Flies", "Shoots"], "Fire", 4.0, 200.0, accent=WHITE,
    payload=R("ghast fireball", "Ball", ["Explodes"], "Fire", 0.7, 3.0), temp="Fearful", inten=0.6), MC)
E(["wither", "the wither", "wither boss"],
  R("Wither", "Blob", ["Flies", "Shoots"], "Shadow", 3.5, 300.0, accent=BLACK,
    payload=R("wither skull", "Ball", ["Explodes", "Homing"], "Shadow", 0.5, 2.0), temp="Chaotic", inten=0.9), MC)
E(["iron golem", "minecraft golem"],
  R("Iron Golem", "Biped", ["Grab", "Heavy", "Launch"], "Earth", 2.7, 1000.0, accent=STEEL, temp="Protective", inten=0.8), MC)
E(["warden", "minecraft warden"],
  R("Warden", "Biped", ["Beam", "Heavy"], "Shadow", 2.9, 1500.0, accent=CYAN, temp="Aggressive", inten=0.9), MC)
E(["mace", "minecraft mace", "heavy core mace"],
  R("Mace", "Hammer", ["Blunt", "Heavy", "Stomp"], "Wind", 1.0, 30.0), MC)
E(["wind charge", "breeze charge"],
  R("Wind Charge", "Ball", ["Launch", "Repel"], "Wind", 0.4, 0.5, accent=WHITE), MC)
E(["slime block", "minecraft slime block"],
  R("Slime Block", "Trampoline", ["Bounce", "Sticky"], "Water", 1.0, 20.0, accent=GREEN), MC)
E(["minecart", "mine cart", "minecraft minecart"],
  R("Minecart", "Vehicle", ["Fast", "Piloted"], "Light", 1.0, 100.0, feat=["Wheels"]), MC)

# ================================================================== FORTNITE
FN = "Epic Games (Fortnite)"
E(["battle bus", "the battle bus", "fortnite bus", "fortnite battle bus"],
  R("Battle Bus", "Vehicle", ["Flies", "Heavy", "Piloted"], "Light", 5.0, 9000.0, accent=BLUE,
    feat=["Wheels", "Windows"]), FN, get("battle_bus"))
E(["launch pad", "fortnite launch pad", "launchpad"],
  R("Launch Pad", "Trampoline", ["Bounce", "Launch"], "Wind", 1.6, 20.0), FN)
E(["fortnite glider", "glider", "umbrella glider"],
  R("Glider", "Wings", ["Glide", "SlowFall"], "Wind", 2.0, 3.0), FN)
E(["shockwave grenade", "shockwave nade", "shockwave"],
  R("Shockwave Grenade", "Bomb", ["Launch", "Repel"], "Electric", 0.4, 1.0, accent=PURPLE), FN)
E(["fortnite pickaxe", "harvesting tool", "default pickaxe"],
  R("Harvesting Tool", "Axe", ["Blunt", "Fast"], "Light", 1.0, 3.0), FN)
E(["rift", "rift-to-go", "rift to go"],
  R("Rift", "Pillar", ["Launch", "Portal"], "Psychic", 2.0, 5.0, accent=PURPLE), FN)
E(["grappler", "fortnite grappler", "grapple blade"],
  R("Grappler", "Whip", ["Pull"], "Light", 1.0, 2.0), FN)

# ================================================================== HALO
H = "Microsoft (Halo)"
E(["energy sword", "halo sword", "plasma sword", "elite sword", "e-sword", "esword"],
  R("Energy Sword", "Sword", ["Blade", "Fast", "Pierce"], "Electric", 1.3, 2.0, accent=CYAN), H, get("energy_sword"))
E(["gravity hammer", "grav hammer", "brute hammer", "halo hammer"],
  R("Gravity Hammer", "Hammer", ["Blunt", "Heavy", "Repel"], "Psychic", 2.4, 60.0, accent=CYAN), H, get("gravity_hammer"))
E(["warthog", "halo warthog", "hog", "m12 warthog", "puma"],
  R("Warthog", "Vehicle", ["Fast", "Piloted", "Shoots"], "Light", 3.0, 3000.0, accent=(0.5, 0.55, 0.35),
    feat=["Turret", "Wheels"]), H, get("warthog"))
E(["plasma grenade", "sticky grenade", "plasma nade", "halo grenade"],
  R("Plasma Grenade", "Bomb", ["Explodes", "Sticky"], "Electric", 0.4, 1.0, accent=BLUE), H, get("plasma_grenade"))
E(["needler", "halo needler", "needle gun"],
  R("Needler", "Gun", ["Shoots"], "Psychic", 0.8, 4.0, accent=PINK,
    payload=R("needle", "Ball", ["Explodes", "Homing", "Sticky"], "Psychic", 0.3, 0.3, feat=["Spikes"])), H, get("needler"))
E(["pelican", "halo pelican", "pelican dropship"],
  R("Pelican", "Vehicle", ["Flies", "Heavy", "Piloted"], "Light", 6.0, 12000.0, feat=["Cockpit", "Jet"]), H)
E(["scorpion tank", "halo scorpion", "halo tank"],
  R("Scorpion", "Vehicle", ["Heavy", "Piloted", "Shoots"], "Fire", 3.5, 40000.0, feat=["Treads", "Turret"],
    payload=R("tank shell", "Ball", ["Explodes"], "Fire", 0.5, 20.0)), H)
E(["banshee", "halo banshee"],
  R("Banshee", "Vehicle", ["Fast", "Flies", "Piloted", "Shoots"], "Electric", 2.5, 900.0, accent=PURPLE), H)
E(["spartan laser", "splaser"],
  R("Spartan Laser", "Cannon", ["Beam", "Pierce"], "Fire", 1.6, 20.0, accent=RED), H)
E(["master chief", "spartan", "spartan-117", "john-117"],
  R("Master Chief", "Biped", ["Heavy", "Shoots"], "Light", 2.1, 450.0, accent=GOLD, temp="Protective", inten=0.7), H)
E(["grunt", "grunts", "halo grunt", "unggoy"],
  R("Grunt", "Biped", ["Shoots", "Swarm"], "Electric", 1.0, 30.0, 5, temp="Fearful", inten=0.9), H)

# ================================================================== DOOM
D = "id Software (Doom)"
E(["bfg", "bfg 9000", "bfg9000", "big fucking gun", "bfg-9000", "bfg 10000"],
  R("BFG 9000", "Cannon", ["Heavy", "Shoots"], "Atomic", 1.6, 30.0, accent=GREEN,
    payload=R("BFG ball", "Ball", ["Beam", "Explodes"], "Atomic", 1.0, 5.0)), D, get("bfg_9000"))
E(["chainsaw", "doom chainsaw", "chain saw"],
  R("Chainsaw", "Sword", ["Blade", "Spin"], "Light", 1.2, 8.0, accent=RED, feat=["Saw"]), D, get("doom_chainsaw"))
E(["cacodemon", "cacodemons", "caco"],
  R("Cacodemon", "Blob", ["Flies", "Shoots"], "Electric", 2.4, 400.0, accent=BLUE,
    payload=R("caco ball", "Ball", [], "Electric", 0.5, 1.0), temp="Aggressive", inten=0.7), D, get("cacodemon"))
E(["super shotgun", "doom shotgun", "double barrel shotgun", "meat hook"],
  R("Super Shotgun", "Gun", ["Heavy", "Pull", "Shoots"], "Light", 1.0, 5.0, feat=["Barrels"]), D)
E(["crucible", "crucible blade", "doom crucible"],
  R("Crucible", "Sword", ["Blade", "Pierce"], "Fire", 1.3, 3.0, accent=RED), D)
E(["cyberdemon", "cyber demon"],
  R("Cyberdemon", "Biped", ["Heavy", "Shoots", "Stomp"], "Fire", 4.0, 3000.0, payload=rocket(homing=False),
    temp="Aggressive", inten=0.9), D)
E(["pinky", "pinky demon", "doom pinky"],
  R("Pinky", "Quadruped", ["Charge", "Heavy"], "Light", 1.6, 400.0, accent=PINK, temp="Aggressive", inten=0.9), D)
E(["lost soul", "lost souls", "doom lost soul"],
  R("Lost Soul", "Blob", ["Charge", "Flies", "Swarm"], "Fire", 0.7, 5.0, 4, temp="Aggressive", inten=0.9), D)

# ================================================================== PORTAL
P = "Valve (Portal)"
E(["portal gun", "aperture science handheld portal device", "portal device", "portalgun", "asphd"],
  R("Portal Gun", "Gun", ["Portal"], "Light", 1.0, 3.0, accent=(0.2, 0.6, 1.0)), P, get("portal_gun"))
E(["companion cube", "weighted companion cube", "portal cube", "weighted storage cube", "storage cube"],
  R("Companion Cube", "Anvil", ["DropFromSky", "Heavy"], "Light", 1.0, 60.0, accent=PINK), P, get("companion_cube"))
E(["portal turret", "sentry turret", "aperture turret"],
  R("Aperture Turret", "Pillar", ["Shoots"], "Light", 1.2, 20.0, accent=RED, feat=["Barrels"]), P)
E(["aerial faith plate", "faith plate"],
  R("Aerial Faith Plate", "Ramp", ["Launch"], "Light", 1.5, 50.0), P)
E(["repulsion gel", "blue gel", "bounce gel"],
  R("Repulsion Gel", "Trampoline", ["Bounce", "Sticky"], "Water", 1.5, 5.0, accent=BLUE), P)
E(["propulsion gel", "orange gel", "speed gel"],
  R("Propulsion Gel", "Ramp", ["Fast", "Sticky"], "Water", 1.5, 5.0, accent=ORANGE), P)
E(["excursion funnel", "tractor beam funnel"],
  R("Excursion Funnel", "Staff", ["Attract", "Beam"], "Psychic", 1.2, 3.0, accent=BLUE), P)
E(["hard light bridge", "light bridge"],
  R("Hard Light Bridge", "Platform", ["Barrier"], "Light", 4.0, 50.0), P)
E(["glados"],
  R("GLaDOS", "Serpent", ["Beam", "Shoots"], "Poison", 3.0, 800.0, temp="Chaotic", inten=0.7), P)

# ================================================================== HALF-LIFE
HL = "Valve (Half-Life)"
E(["gravity gun", "zero point energy field manipulator", "gravgun", "grav gun", "physcannon", "physgun",
   "physics gun"],
  R("Gravity Gun", "Gun", ["Attract", "Grab", "Repel"], "Psychic", 1.2, 6.0, accent=ORANGE), HL, get("gravity_gun"))
E(["crowbar", "freeman's crowbar", "gordon's crowbar"],
  R("Crowbar", "Hammer", ["Blunt", "Fast"], "Light", 0.9, 3.0, accent=RED), HL, get("crowbar"))
E(["headcrab", "head crab", "headcrabs"],
  R("Headcrab", "Arachnid", ["Charge", "Sticky"], "Poison", 0.5, 3.0, 3, accent=(0.9, 0.8, 0.6), temp="Aggressive", inten=0.7),
  HL, get("headcrab"))
E(["strider", "combine strider"],
  R("Strider", "Arachnid", ["Beam", "Shoots", "Stomp"], "Psychic", 4.0, 3000.0, temp="Aggressive", inten=0.6), HL)
E(["manhack", "manhacks"],
  R("Manhack", "Disc", ["Blade", "Flies", "Homing", "Spin"], "Light", 0.5, 2.0, 3, feat=["Saw"],
    temp="Aggressive", inten=0.9), HL)
E(["barnacle", "half-life barnacle"],
  R("Barnacle", "Pillar", ["Grab", "Pull"], "Poison", 1.5, 20.0, temp="Lazy", inten=0.8), HL)
E(["antlion", "antlions"],
  R("Antlion", "Insect", ["Flies", "Swarm"], "Earth", 1.0, 40.0, 5, temp="Aggressive", inten=0.7), HL)

# ================================================================== SOULSBORNE / ELDEN RING
FS = "FromSoftware"
E(["moonlight greatsword", "moonlight great sword", "moonlight sword", "moonlight"],
  R("Moonlight Greatsword", "Sword", ["Blade", "Shoots"], "Ice", 2.2, 10.0, accent=CYAN,
    payload=R("moonlight wave", "Disc", ["Blade", "Pierce"], "Ice", 1.5, 1.0)), FS + " (Dark Souls/Elden Ring)", get("moonlight_greatsword"))
E(["rivers of blood", "rob", "corpse piler", "rivers of blood katana"],
  R("Rivers of Blood", "Sword", ["Blade", "Fast", "Shoots"], "Fire", 1.3, 3.0, accent=RED,
    payload=R("blood slash", "Disc", ["Blade", "Fast"], "Fire", 1.0, 0.5)), FS + " (Elden Ring)", get("rivers_of_blood"))
E(["malenia", "malenia blade of miquella", "waterfowl dance", "let me solo her"],
  R("Malenia", "Biped", ["Blade", "Fast", "Spin"], "Poison", 2.2, 70.0, accent=(0.95, 0.5, 0.2),
    temp="Aggressive", inten=1.0), FS + " (Elden Ring)")
E(["radahn", "starscourge radahn", "general radahn"],
  R("Starscourge Radahn", "Biped", ["Attract", "DropFromSky", "Heavy"], "Psychic", 4.0, 3000.0, accent=RED,
    temp="Aggressive", inten=0.9), FS + " (Elden Ring)")
E(["torrent", "spectral steed", "spirit steed"],
  R("Torrent", "Quadruped", ["Fast", "Piloted"], "Light", 2.0, 400.0), FS + " (Elden Ring)")
E(["comet azur", "azur's comet"],
  R("Comet Azur", "Staff", ["Beam"], "Psychic", 1.4, 2.0, accent=BLUE), FS + " (Elden Ring)")
E(["rock sling"],
  R("Rock Sling", "Staff", ["DropFromSky"], "Earth", 1.2, 2.0, payload=R("rock", "Boulder", ["Blunt"], "Earth", 0.8, 40.0, 3)),
  FS + " (Elden Ring)")
E(["moonveil", "moonveil katana"],
  R("Moonveil", "Sword", ["Blade", "Fast", "Shoots"], "Psychic", 1.3, 3.0, accent=BLUE,
    payload=R("transient moonlight", "Disc", ["Blade", "Fast"], "Psychic", 1.0, 0.5)), FS + " (Elden Ring)")
E(["ornstein", "dragon slayer ornstein", "dragonslayer ornstein"],
  R("Ornstein", "Biped", ["Charge", "Fast", "Pierce"], "Electric", 2.4, 200.0, accent=GOLD, temp="Aggressive", inten=0.8),
  FS + " (Dark Souls)")
E(["smough", "executioner smough"],
  R("Smough", "Biped", ["Blunt", "Heavy", "Stomp"], "Earth", 3.2, 900.0, accent=GOLD, temp="Aggressive", inten=0.8),
  FS + " (Dark Souls)")
E(["artorias", "knight artorias", "artorias the abysswalker"],
  R("Artorias", "Biped", ["Blade", "Fast"], "Shadow", 2.3, 150.0, accent=BLUE, temp="Aggressive", inten=1.0),
  FS + " (Dark Souls)")
E(["great grey wolf sif", "sif"],
  R("Sif", "Quadruped", ["Blade", "Charge"], "Light", 3.5, 1500.0, temp="Protective", inten=0.7), FS + " (Dark Souls)")
E(["havel's greatshield", "havels greatshield", "havel shield", "havel"],
  R("Havel's Greatshield", "Shield", ["Barrier", "Guard", "Heavy"], "Earth", 1.8, 60.0), FS + " (Dark Souls)")
E(["dragonslayer greatbow", "dragonslayer bow"],
  R("Dragonslayer Greatbow", "Bow", ["Heavy", "Pierce"], "Light", 2.4, 20.0), FS + " (Dark Souls)")
E(["saw cleaver", "hunter's saw cleaver"],
  R("Saw Cleaver", "Axe", ["Blade", "Spin"], "Light", 1.2, 5.0, feat=["Saw"]), FS + " (Bloodborne)")
E(["ludwig's holy blade", "holy moonlight sword"],
  R("Ludwig's Holy Blade", "Sword", ["Blade", "Heavy"], "Light", 2.0, 12.0), FS + " (Bloodborne)")
E(["blunderbuss", "hunter blunderbuss"],
  R("Hunter Blunderbuss", "Gun", ["Repel", "Shoots"], "Light", 0.9, 5.0), FS + " (Bloodborne)")
E(["mimic tear", "mimic tear ash"],
  R("Mimic Tear", "Biped", ["Blade"], "Water", 1.8, 80.0, accent=STEEL, temp="Protective", inten=0.8), FS + " (Elden Ring)")

# ================================================================== MONSTER HUNTER
MH = "Capcom (Monster Hunter)"
E(["rathalos", "king of the skies", "fire wyvern", "silver rathalos"],
  R("Rathalos", "Bird", ["ArticulatedTail", "Breath", "Flies"], "Fire", 4.0, 3000.0, accent=RED,
    temp="Aggressive", inten=0.8), MH, get("rathalos"))
E(["arkveld", "chained arkveld", "guardian arkveld"],
  R("Arkveld", "Quadruped", ["Charge", "Link", "Pull"], "Electric", 4.0, 5000.0, accent=GOLD, feat=["Chains"],
    temp="Aggressive", inten=0.9), MH)
E(["zinogre", "thunder wolf wyvern"],
  R("Zinogre", "Quadruped", ["Charge", "Stomp"], "Electric", 3.5, 3000.0, accent=YELLOW, temp="Aggressive", inten=0.8), MH)
E(["diablos", "black diablos"],
  R("Diablos", "Quadruped", ["Charge", "Heavy"], "Earth", 3.8, 6000.0, temp="Aggressive", inten=1.0), MH)
E(["rajang"], R("Rajang", "Biped", ["Beam", "Grab"], "Electric", 3.0, 2000.0, temp="Aggressive", inten=1.0), MH)
E(["gunlance", "wyvern's fire"],
  R("Gunlance", "Spear", ["Pierce", "Shoots"], "Fire", 2.0, 20.0,
    payload=R("gunlance shell", "Ball", ["Explodes"], "Fire", 0.4, 1.0)), MH)
E(["monster hunter great sword", "mh great sword"],
  R("Hunter's Great Sword", "Sword", ["Blade", "Heavy"], "Light", 2.2, 25.0), MH)
E(["hunting horn", "mh hunting horn", "monster hunter hunting horn"], R("Hunting Horn", "Hammer", ["Blunt", "Repel"], "Wind", 1.6, 20.0), MH)
E(["insect glaive", "kinsect"],
  R("Insect Glaive", "Staff", ["Launch", "Shoots"], "Light", 1.8, 5.0,
    payload=R("kinsect", "Insect", ["Flies", "Homing", "Returning"], "Light", 0.3, 0.5)), MH)
E(["palico", "felyne", "palicoes"],
  R("Palico", "Biped", ["Shoots"], "Light", 0.6, 8.0, payload=R("barrel bomb", "Bomb", ["Explodes"], "Fire", 0.4, 2.0),
    temp="Protective", inten=0.8), MH)
E(["large barrel bomb", "barrel bomb", "large barrel bomb+"],
  R("Large Barrel Bomb", "Bomb", ["Explodes", "Heavy"], "Fire", 1.0, 30.0, accent=RED), MH)
E(["wirebug", "wire bug"], R("Wirebug", "Whip", ["Launch", "Pull"], "Light", 1.0, 1.0), MH)

# ================================================================== FINAL FANTASY
FF = "Square Enix (Final Fantasy)"
E(["buster sword", "cloud's sword", "clouds sword", "cloud sword", "buster blade", "cloud strife sword"],
  R("Buster Sword", "Sword", ["Blade", "Heavy"], "Light", 2.0, 30.0, accent=STEEL), FF, get("buster_sword"))
E(["gunblade", "gun blade", "revolver gunblade", "squall's gunblade", "lionheart"],
  R("Gunblade", "Sword", ["Blade", "Shoots"], "Fire", 1.4, 6.0, accent=STEEL,
    payload=R("trigger burst", "Ball", ["Explodes", "Fast"], "Fire", 0.3, 0.5)), FF, get("gunblade"))
E(["chocobo", "chocobos", "ride a chocobo", "big bird"],
  R("Chocobo", "Biped", ["Fast", "Piloted"], "Light", 2.2, 200.0, accent=ORANGE, temp="Playful", inten=0.5), FF, get("chocobo"))
E(["masamune", "sephiroth's sword", "sephiroth sword"],
  R("Masamune", "Sword", ["Blade", "Fast", "Pierce"], "Shadow", 2.4, 4.0, accent=STEEL), FF)
E(["bahamut", "megaflare", "bahamut zero"],
  R("Bahamut", "Bird", ["ArticulatedTail", "Beam", "Flies"], "Atomic", 4.0, 5000.0, accent=BLUE, temp="Aggressive", inten=0.8), FF)
E(["moogle", "moogles", "kupo"],
  R("Moogle", "Blob", ["Flies"], "Light", 0.8, 5.0, accent=RED, temp="Playful", inten=0.8), FF)
E(["cactuar", "1000 needles", "cactuars"],
  R("Cactuar", "Biped", ["Fast", "Shoots"], "Earth", 1.0, 20.0, accent=GREEN,
    payload=R("needle", "Ball", ["Pierce"], "Earth", 0.2, 0.2, 10), temp="Playful", inten=0.8), FF)
E(["tonberry", "tonberries"],
  R("Tonberry", "Biped", ["Blade", "Pierce"], "Light", 0.9, 30.0, accent=YELLOW, temp="Sneaky", inten=1.0), FF)
E(["malboro", "bad breath", "marlboro"],
  R("Malboro", "Blob", ["Breath", "Constrict"], "Poison", 2.4, 600.0, accent=GREEN, temp="Lazy", inten=0.6), FF)
E(["behemoth", "king behemoth", "ff behemoth"],
  R("Behemoth", "Quadruped", ["Charge", "DropFromSky", "Heavy"], "Fire", 4.0, 8000.0, accent=PURPLE,
    temp="Aggressive", inten=0.8), FF)
E(["highwind", "final fantasy airship", "ff airship"],
  R("Airship", "Vehicle", ["Flies", "Heavy", "Piloted"], "Wind", 8.0, 20000.0, feat=["Propeller", "Sails"]), FF)
E(["magitek armor", "magitek armour"],
  R("Magitek Armor", "Biped", ["Beam", "Piloted"], "Fire", 3.0, 3000.0, feat=["Armor", "Pipes"]), FF)
E(["shiva", "diamond dust"],
  R("Shiva", "Biped", ["Breath", "Fast"], "Ice", 2.2, 60.0, accent=CYAN, temp="Calm", inten=0.6), FF)
E(["ifrit", "hellfire"],
  R("Ifrit", "Biped", ["Charge", "Explodes"], "Fire", 3.0, 900.0, accent=ORANGE, temp="Aggressive", inten=0.9), FF)

# ================================================================== KINGDOM HEARTS
KH = "Square Enix/Disney (Kingdom Hearts)"
E(["keyblade", "kingdom key", "key blade", "sora's keyblade", "soras keyblade", "strike raid"],
  R("Keyblade", "Sword", ["Blunt", "Returning"], "Light", 1.3, 4.0, accent=GOLD), KH, get("keyblade"))
E(["heartless", "shadow heartless", "heartless shadow"],
  R("Heartless", "Biped", ["Swarm"], "Shadow", 0.7, 5.0, 6, accent=GOLD, temp="Sneaky", inten=0.8), KH)
E(["gummi ship", "gummy ship"],
  R("Gummi Ship", "Vehicle", ["Flies", "Piloted", "Shoots"], "Light", 3.0, 500.0, accent=RED, feat=["Jet"]), KH)

# ================================================================== GOD OF WAR
GW = "Sony Santa Monica (God of War)"
E(["leviathan axe", "leviathan", "kratos axe", "kratos' axe", "kratoss axe", "frost axe"],
  R("Leviathan Axe", "Axe", ["Blade", "Returning"], "Ice", 1.2, 10.0, accent=CYAN), GW, get("leviathan_axe"))
E(["blades of chaos", "chaos blades", "blades of athena", "blades of exile", "kratos blades"],
  R("Blades of Chaos", "Whip", ["Blade", "Pull"], "Fire", 1.2, 6.0, accent=ORANGE, feat=["Chains"]), GW, get("blades_of_chaos"))
E(["guardian shield", "kratos shield", "dauntless shield"],
  R("Guardian Shield", "Shield", ["Barrier", "Guard", "Repel"], "Light", 1.0, 10.0), GW)
E(["draupnir spear", "draupnir"],
  R("Draupnir Spear", "Spear", ["Explodes", "Pierce", "Sticky"], "Electric", 1.8, 5.0, accent=GOLD), GW)
E(["jormungandr", "world serpent", "jörmungandr", "gow jormungandr", "god of war jormungandr"],
  R("Jörmungandr", "Serpent", ["Constrict", "Heavy"], "Water", 20.0, 90000.0, temp="Lazy", inten=0.6), GW)

# ================================================================== METAL GEAR
MG = "Konami (Metal Gear)"
E(["metal gear rex", "rex", "metal gear", "metal gear mech"],
  R("Metal Gear REX", "Kaiju", ["Beam", "Heavy", "Piloted", "Shoots", "Stomp"], "Atomic", 13.0, 60000.0, accent=STEEL,
    payload=rocket("REX missile"), feat=["Armor", "Cockpit"]), MG, get("metal_gear_rex"))
E(["metal gear ray", "ray"],
  R("Metal Gear RAY", "Kaiju", ["ArticulatedTail", "Beam", "Piloted"], "Water", 12.0, 50000.0, feat=["Armor"]), MG)
E(["cardboard box", "snake's box", "snake box", "solid snake box"],
  R("Cardboard Box", "Cage", ["Trap", "Walks"], "Light", 1.0, 2.0, temp="Sneaky", inten=1.0), MG)
E(["fulton", "fulton recovery", "fulton balloon"],
  R("Fulton", "Ball", ["Flies", "Grab"], "Wind", 0.8, 1.0), MG)

# ================================================================== TITANFALL / APEX
TF = "Respawn (Titanfall)"
AP = "Respawn (Apex Legends)"
E(["bt-7274", "bt 7274", "bt", "titan", "titanfall titan", "vanguard titan", "stand by for titanfall"],
  R("BT-7274", "Biped", ["DropFromSky", "Piloted", "Shoots"], "Light", 7.0, 20000.0, accent=(0.35, 0.6, 0.9),
    feat=["Armor", "Cockpit"], temp="Protective", inten=0.9), TF, get("titan_bt"))
E(["titanfall grapple", "pilot grapple", "grapple hook"],
  R("Grapple", "Whip", ["Pull"], "Light", 1.0, 1.0), TF)
E(["smart pistol"], R("Smart Pistol", "Gun", ["Homing", "Shoots"], "Light", 0.5, 1.0), TF)
E(["arc grenade"], R("Arc Grenade", "Bomb", ["Explodes"], "Electric", 0.3, 0.5), TF)
E(["pathfinder grapple", "pathfinder"],
  R("Pathfinder Grapple", "Whip", ["Launch", "Pull"], "Light", 1.0, 1.0), AP)
E(["jump pad", "octane jump pad", "octane pad"],
  R("Jump Pad", "Trampoline", ["Bounce", "Launch"], "Light", 1.5, 20.0, accent=ORANGE), AP)
E(["gibraltar dome", "dome of protection", "gibby dome", "gibby bubble"],
  R("Dome of Protection", "Dome", ["Barrier"], "Light", 5.0, 50.0), AP)
E(["defensive bombardment", "gibby ult", "gibraltar ult"],
  R("Defensive Bombardment", "Bomb", ["DropFromSky", "Explodes"], "Fire", 0.6, 5.0, 8), AP)
E(["wraith portal", "into the void"],
  R("Wraith Portal", "Pillar", ["Portal"], "Shadow", 2.0, 2.0, accent=PURPLE), AP)
E(["black hole grenade", "horizon ult", "nevermind the black hole"],
  R("Black Hole", "Bomb", ["Attract", "Explodes"], "Shadow", 1.0, 5.0, accent=PURPLE), AP)
E(["gravity lift", "horizon lift"],
  R("Gravity Lift", "Pillar", ["Launch", "SlowFall"], "Psychic", 3.0, 10.0), AP)

# ================================================================== OVERWATCH / MARVEL RIVALS
OW = "Blizzard (Overwatch)"
E(["d.va mech", "dva mech", "d.va", "dva", "meka", "nerf this"],
  R("D.Va Mech", "Biped", ["Explodes", "Piloted", "Shoots", "Thrust"], "Light", 3.0, 3000.0, accent=PINK,
    feat=["Cockpit", "Jet"]), OW)
E(["rocket hammer", "reinhardt hammer", "reinhardt's hammer", "earthshatter"],
  R("Rocket Hammer", "Hammer", ["Blunt", "Heavy", "Stomp"], "Fire", 2.0, 60.0, feat=["Jet"]), OW)
E(["reinhardt shield", "barrier field", "rein shield"],
  R("Barrier Field", "Wall", ["Barrier", "Guard"], "Light", 3.0, 30.0, accent=BLUE), OW)
E(["rip-tire", "riptire", "rip tire", "junkrat tire", "junkrat ult"],
  R("RIP-Tire", "Vehicle", ["Explodes", "Fast", "Piloted"], "Fire", 1.2, 40.0, feat=["Spikes", "Wheels"]), OW)
E(["concussion mine", "junkrat mine"],
  R("Concussion Mine", "Bomb", ["Launch", "Repel", "Sticky"], "Fire", 0.5, 1.0), OW)
E(["ice wall", "mei wall", "mei ice wall"],
  R("Ice Wall", "Wall", ["Barrier"], "Ice", 3.0, 200.0, accent=CYAN), OW)
E(["dragonstrike", "hanzo dragons", "hanzo ult", "ryuu ga waga teki wo kurau"],
  R("Dragonstrike", "Serpent", ["Fast", "Flies", "Swarm"], "Electric", 3.0, 100.0, 2, accent=CYAN), OW)
E(["chain hook", "roadhog hook", "hog hook"],
  R("Chain Hook", "Whip", ["Grab", "Pull"], "Light", 1.0, 5.0, feat=["Chains"]), OW)
E(["graviton surge", "zarya ult"],
  R("Graviton Surge", "Bomb", ["Attract"], "Psychic", 0.8, 2.0, accent=PINK), OW)
E(["tesla cannon", "winston gun"],
  R("Tesla Cannon", "Gun", ["Beam"], "Electric", 1.0, 5.0, accent=CYAN), OW)
E(["pulse bomb", "tracer bomb"],
  R("Pulse Bomb", "Bomb", ["Explodes", "Sticky"], "Electric", 0.3, 0.5, accent=BLUE), OW)
MR = "NetEase/Marvel (Marvel Rivals)"
E(["jeff the land shark", "jeff shark"],
  R("Jeff the Land Shark", "Fish", ["Grab", "Walks"], "Water", 1.0, 30.0, accent=WHITE, temp="Playful", inten=0.9), MR)

# ================================================================== VALORANT / LEAGUE / CS
V = "Riot Games (Valorant)"
L = "Riot Games (League of Legends)"
E(["shock bolt", "sova arrow", "recon bolt", "sova bow"],
  R("Sova's Bow", "Bow", ["Ricochet", "Shoots"], "Electric", 1.4, 2.0, accent=BLUE), V)
E(["boom bot", "raze boom bot", "boombot"],
  R("Boom Bot", "Vehicle", ["Explodes", "Homing", "Ricochet", "Walks"], "Fire", 0.5, 5.0, accent=ORANGE, feat=["Wheels"]), V)
E(["showstopper", "raze ult", "raze rocket launcher"],
  R("Showstopper", "Cannon", ["Shoots"], "Fire", 1.4, 20.0, payload=rocket(homing=False)), V)
E(["blade storm", "jett knives", "jett ult"],
  R("Blade Storm", "Disc", ["Blade", "Fast", "Returning"], "Wind", 0.4, 0.5, 5), V)
E(["sage wall", "barrier orb"], R("Sage Wall", "Wall", ["Barrier"], "Ice", 3.0, 200.0, accent=CYAN), V)
E(["orbital strike", "brimstone ult", "brimstone orbital strike"],
  R("Orbital Strike", "Pillar", ["Beam", "DropFromSky"], "Fire", 6.0, 10.0, accent=ORANGE), V)
E(["blitzcrank", "rocket grab", "blitz hook"],
  R("Blitzcrank", "Biped", ["Grab", "Pull", "Thrust"], "Electric", 2.2, 800.0, accent=GOLD, temp="Aggressive", inten=0.7), L)
E(["thresh hook", "death sentence", "thresh lantern", "thresh"],
  R("Death Sentence", "Whip", ["Link", "Pull"], "Shadow", 1.4, 3.0, accent=GREEN, feat=["Chains"]), L)
E(["tibbers", "annie's bear", "annie bear"],
  R("Tibbers", "Biped", ["Heavy", "Stomp"], "Fire", 3.0, 800.0, temp="Aggressive", inten=0.9), L)
E(["super mega death rocket", "jinx rocket", "jinx ult", "fishbones"],
  R("Super Mega Death Rocket", "Rocket", ["Explodes", "Fast"], "Fire", 2.0, 30.0, accent=PINK), L)
E(["unstoppable force", "malphite ult", "malphite"],
  R("Unstoppable Force", "Boulder", ["Charge", "Heavy", "Launch"], "Earth", 2.4, 3000.0), L)
E(["baron nashor", "baron"],
  R("Baron Nashor", "Serpent", ["Breath", "Grab"], "Poison", 4.0, 9000.0, accent=PURPLE, temp="Lazy", inten=0.6), L)
E(["teemo mushroom", "noxious trap", "teemo shroom"],
  R("Noxious Trap", "Bomb", ["Explodes", "Trap"], "Poison", 0.4, 0.5, 3, temp="Sneaky", inten=1.0), L)
CS = "Valve (Counter-Strike)"
E(["awp", "dragon lore", "awp dragon lore"],
  R("AWP", "Gun", ["Heavy", "Pierce", "Shoots"], "Light", 1.2, 6.0, accent=GREEN), CS)
E(["c4", "the bomb has been planted", "cs bomb", "c4 bomb"],
  R("C4", "Bomb", ["Explodes", "Heavy", "Sticky"], "Fire", 0.5, 2.0), CS)

# ================================================================== GTA / CALL OF DUTY / BATTLEFIELD / DESTINY
GTA = "Rockstar (Grand Theft Auto)"
E(["oppressor mk ii", "oppressor mk2", "oppressor", "flying bike", "griefer bike"],
  R("Oppressor Mk II", "Vehicle", ["Fast", "Flies", "Piloted", "Shoots"], "Fire", 2.0, 300.0,
    payload=rocket("homing missile"), feat=["Jet", "Wheels"]), GTA)
E(["rhino tank", "gta tank", "rhino"],
  R("Rhino Tank", "Vehicle", ["Heavy", "Piloted", "Shoots"], "Fire", 3.0, 60000.0, feat=["Treads", "Turret"]), GTA)
E(["sticky bomb", "gta sticky bomb"],
  R("Sticky Bomb", "Bomb", ["Explodes", "Sticky"], "Fire", 0.3, 1.0), GTA)
E(["gta minigun", "minigun"],
  R("Minigun", "Gun", ["Heavy", "Shoots", "Spin"], "Light", 1.4, 20.0, feat=["Barrels"]), GTA)
E(["gta 6 gator", "florida gator", "gta alligator"],
  R("Vice City Gator", "Quadruped", ["Grab", "Spin"], "Water", 1.0, 300.0, temp="Chaotic", inten=0.8), GTA + " VI")
COD = "Activision (Call of Duty)"
E(["ray gun", "raygun", "zombies ray gun", "ray gun mark ii"],
  R("Ray Gun", "Gun", ["Shoots"], "Atomic", 0.6, 2.0, accent=GREEN,
    payload=R("ray gun bolt", "Ball", ["Explodes", "Fast"], "Atomic", 0.3, 0.3)), COD + " Zombies")
E(["thundergun", "thunder gun"],
  R("Thundergun", "Cannon", ["Launch", "Repel"], "Wind", 1.2, 10.0), COD + " Zombies")
E(["wunderwaffe", "wunderwaffe dg-2", "wonder weapon"],
  R("Wunderwaffe DG-2", "Gun", ["Beam", "Ricochet"], "Electric", 1.0, 6.0, accent=BLUE), COD + " Zombies")
E(["care package", "supply drop"],
  R("Care Package", "Anvil", ["DropFromSky", "Heavy"], "Light", 1.5, 200.0), COD)
E(["predator missile", "hellstorm missile"],
  R("Predator Missile", "Rocket", ["DropFromSky", "Explodes", "Homing"], "Fire", 1.5, 50.0), COD)
E(["rc-xd", "rcxd", "rc car bomb"],
  R("RC-XD", "Vehicle", ["Explodes", "Fast", "Piloted"], "Fire", 0.5, 5.0, feat=["Wheels"]), COD)
E(["tactical nuke", "moab", "nuke killstreak"],
  R("Tactical Nuke", "Bomb", ["DropFromSky", "Explodes", "Heavy"], "Atomic", 2.0, 500.0), COD)
DS = "Bungie (Destiny)"
E(["gjallarhorn", "wolfpack rounds"],
  R("Gjallarhorn", "Cannon", ["Shoots"], "Fire", 1.6, 20.0, accent=GOLD, payload=R("wolfpack round", "Rocket", ["Explodes", "Homing"], "Fire", 0.4, 1.0, 3)), DS)
E(["ghost shell", "destiny ghost"], R("Ghost", "Blob", ["Flies"], "Light", 0.4, 1.0, temp="Protective", inten=0.9), DS)

# ================================================================== SOULSLIKE INDIES / ROGUELIKES
HK = "Team Cherry (Hollow Knight)"
E(["the knight's nail", "old nail", "pure nail", "hollow knight nail", "nail"],
  R("Nail", "Sword", ["Blade", "Bounce"], "Light", 0.9, 1.0), HK)
E(["hornet's needle", "hornet needle", "silksong needle", "needle"],
  R("Hornet's Needle", "Spear", ["Pierce", "Pull", "Returning"], "Light", 1.0, 1.0, accent=RED), HK + ": Silksong")
E(["hornet", "hornet silksong", "princess protector", "git gud"],
  R("Hornet", "Biped", ["Fast", "Pull"], "Light", 1.2, 20.0, accent=RED, temp="Aggressive", inten=0.8), HK + ": Silksong")
E(["vengeful spirit", "shade soul"],
  R("Vengeful Spirit", "Ball", ["Fast", "Pierce"], "Shadow", 0.8, 1.0), HK)
HD = "Supergiant (Hades)"
E(["aegis", "shield of chaos", "zagreus shield"],
  R("Shield of Chaos", "Shield", ["Guard", "Returning", "Ricochet"], "Light", 1.2, 8.0, accent=GOLD), HD)
E(["varatha", "eternal spear"], R("Varatha", "Spear", ["Pierce", "Returning"], "Light", 1.8, 4.0), HD)
E(["stygius", "stygian blade"], R("Stygius", "Sword", ["Blade"], "Shadow", 1.1, 3.0), HD)
E(["cerberus", "three-headed dog", "three headed dog", "hades cerberus"],
  R("Cerberus", "Quadruped", ["Breath", "Charge"], "Fire", 3.0, 2000.0, temp="Protective", inten=0.9), HD)
E(["witch's staff", "descura", "melinoe staff"],
  R("Witch's Staff", "Staff", ["Beam"], "Psychic", 1.6, 3.0, accent=PURPLE), HD + " II")
TR = "Re-Logic (Terraria)"
E(["zenith", "terraria zenith"],
  R("Zenith", "Sword", ["Blade", "Shoots"], "Light", 1.4, 3.0, accent=GREEN,
    payload=R("spectral sword", "Sword", ["Blade", "Flies", "Homing"], "Light", 1.0, 1.0, 4)), TR)
E(["terraprisma"], R("Terraprisma", "Sword", ["Flies", "Homing"], "Light", 1.0, 1.0, 4, temp="Protective", inten=0.8), TR)
E(["eye of cthulhu"],
  R("Eye of Cthulhu", "Blob", ["Charge", "Flies"], "Shadow", 3.0, 300.0, accent=RED, temp="Aggressive", inten=0.8), TR)
E(["king slime"], R("King Slime", "Blob", ["Bounce", "Heavy", "Stomp"], "Water", 3.0, 1500.0, accent=BLUE,
                     temp="Playful", inten=0.7), TR)
E(["meowmere"], R("Meowmere", "Sword", ["Blade", "Shoots"], "Light", 1.3, 3.0, accent=PINK,
                  payload=R("rainbow cat", "Ball", ["Bounce", "Ricochet"], "Light", 0.5, 1.0)), TR)
E(["terrarian yoyo", "yoyo", "yo-yo"], R("Terrarian", "Disc", ["Returning", "Shoots", "Spin"], "Light", 0.4, 1.0), TR)
VS = "poncle (Vampire Survivors)"
E(["king bible", "orbiting bible"],
  R("King Bible", "Disc", ["Flies", "Spin"], "Light", 0.5, 1.0, 4, temp="Protective", inten=1.0), VS)
E(["garlic aura", "vampire survivors garlic"], R("Garlic", "Dome", ["Repel"], "Poison", 3.0, 1.0), VS)
E(["runetracer"], R("Runetracer", "Ball", ["Fast", "Ricochet"], "Light", 0.4, 0.5, 2), VS)
BL = "LocalThunk (Balatro)"
E(["jimbo", "balatro joker", "balatro card", "joker card"],
  R("Jimbo", "Disc", ["Blade", "Ricochet"], "Light", 0.5, 0.2, 5, accent=RED, temp="Playful", inten=0.8), BL)

# ================================================================== HELLDIVERS 2
HE = "Arrowhead (Helldivers 2)"
E(["eagle 500kg bomb", "500kg bomb", "500kg", "eagle 500", "500 kg bomb"],
  R("Eagle 500KG Bomb", "Bomb", ["DropFromSky", "Explodes", "Heavy"], "Fire", 2.0, 500.0), HE)
E(["orbital precision strike", "ops", "orbital strike helldivers"],
  R("Orbital Precision Strike", "Bomb", ["DropFromSky", "Explodes", "Fast"], "Fire", 1.0, 100.0), HE)
E(["orbital laser", "helldivers laser"],
  R("Orbital Laser", "Pillar", ["Beam", "DropFromSky", "Homing"], "Fire", 8.0, 10.0, accent=RED), HE)
E(["eagle airstrike", "airstrike", "eagle strike"],
  R("Eagle Airstrike", "Bomb", ["DropFromSky", "Explodes"], "Fire", 0.6, 20.0, 6), HE)
E(["hellpod", "hell pod", "reinforce", "helldiver drop pod"],
  R("Hellpod", "Rocket", ["DropFromSky", "Heavy"], "Fire", 2.0, 800.0), HE)
E(["hellbomb", "hell bomb"], R("Hellbomb", "Bomb", ["Explodes", "Heavy"], "Atomic", 1.6, 400.0), HE)
E(["exosuit", "patriot exosuit", "emancipator exosuit", "helldivers mech"],
  R("Patriot Exosuit", "Biped", ["Piloted", "Shoots"], "Fire", 3.0, 5000.0, payload=rocket(), feat=["Armor", "Cockpit"]), HE)
E(["bile titan", "bile titans"],
  R("Bile Titan", "Kaiju", ["Breath", "Stomp"], "Poison", 12.0, 30000.0, temp="Aggressive", inten=0.8), HE)
E(["terminid charger", "charger"],
  R("Charger", "Quadruped", ["Charge", "Heavy"], "Earth", 2.5, 3000.0, feat=["Armor"], temp="Aggressive", inten=1.0), HE)
E(["guard dog", "guard dog rover", "rover"],
  R("Guard Dog", "Blob", ["Flies", "Shoots"], "Light", 0.5, 5.0, temp="Protective", inten=1.0, feat=["Rotor"]), HE)
E(["jump pack"], R("Jump Pack", "Jetpack", ["Launch", "Thrust"], "Fire", 1.0, 20.0), HE)
E(["terminid", "terminids", "bugs helldivers"],
  R("Terminid", "Insect", ["Swarm"], "Poison", 1.0, 20.0, 8, temp="Aggressive", inten=0.8), HE)

# ================================================================== PALWORLD
PW = "Pocketpair (Palworld)"
E(["jetragon"], R("Jetragon", "Bird", ["Fast", "Flies", "Piloted", "Shoots"], "Fire", 4.0, 2000.0, payload=rocket("jetragon missile")), PW)
E(["lamball"], R("Lamball", "Quadruped", ["Bounce"], "Light", 0.7, 10.0, 3, accent=WHITE, temp="Playful", inten=0.6), PW)
E(["depresso"], R("Depresso", "Biped", ["Heavy"], "Shadow", 0.7, 10.0, 2, accent=PURPLE, temp="Lazy", inten=1.0), PW)
E(["grizzbolt"], R("Grizzbolt", "Biped", ["Piloted", "Shoots"], "Electric", 2.8, 800.0, accent=YELLOW, feat=["Barrels"]), PW)
E(["pengullet", "pengullet launcher", "penguin rocket launcher"],
  R("Pengullet", "Rocket", ["Explodes", "Homing"], "Ice", 0.8, 5.0, accent=BLUE, temp="Chaotic", inten=0.6), PW)
E(["chillet"], R("Chillet", "Serpent", ["Breath", "Piloted"], "Ice", 1.8, 200.0, accent=CYAN), PW)
E(["anubis", "palworld anubis"], R("Anubis", "Biped", ["Blade", "Fast"], "Earth", 2.4, 200.0, accent=GOLD), PW)

# ================================================================== AMONG US / RETRO / PARTY
AU = "Innersloth (Among Us)"
E(["crewmate", "among us", "amogus", "among us crewmate", "sus", "crewmates"],
  R("Crewmate", "Biped", ["Walks"], "Light", 1.2, 20.0, accent=CYAN, temp="Playful", inten=0.5), AU, get("crewmate"))
E(["impostor", "imposter", "among us impostor", "among us imposter"],
  R("Impostor", "Biped", ["Grab", "Portal"], "Shadow", 1.2, 20.0, accent=RED, temp="Sneaky", inten=1.0), AU)
E(["among us vent", "vent"], R("Vent", "Platform", ["Portal"], "Shadow", 1.0, 20.0), AU)
E(["emergency meeting", "emergency button", "emergency meeting button"],
  R("Emergency Meeting", "Pillar", ["Attract"], "Psychic", 1.2, 50.0, accent=RED), AU)
NM = "Bandai Namco (Pac-Man)"
E(["pac-man ghost", "pacman ghost", "blinky", "inky", "pinky ghost", "clyde"],
  R("Pac-Man Ghost", "Blob", ["Flies", "Homing"], "Shadow", 1.2, 1.0, 4, accent=BLUE, temp="Chaotic", inten=0.7), NM, get("pac_man_ghost"))
E(["pac-man", "pacman", "pac man", "waka waka"],
  R("Pac-Man", "Blob", ["Fast", "Grab"], "Light", 1.0, 5.0, accent=YELLOW, temp="Playful", inten=0.8), NM)
E(["tetris block", "tetromino", "t-piece", "t piece", "tetris", "t-spin"],
  R("Tetromino", "Anvil", ["DropFromSky", "Heavy"], "Light", 2.2, 400.0, accent=PURPLE), "Tetris Company (Tetris)", get("tetris_t"))
E(["space invader", "space invaders"],
  R("Space Invader", "Insect", ["Flies", "Shoots", "Swarm"], "Light", 0.8, 5.0, 8, temp="Aggressive", inten=0.5), "Taito (Space Invaders)")
E(["angry bird", "angry birds", "red bird"],
  R("Angry Bird", "Ball", ["Blunt", "Heavy"], "Light", 0.6, 10.0, accent=RED, temp="Aggressive", inten=1.0), "Rovio (Angry Birds)")
E(["bomb bird", "black angry bird"],
  R("Bomb Bird", "Bomb", ["Explodes"], "Fire", 0.8, 12.0, temp="Aggressive", inten=1.0), "Rovio (Angry Birds)")
E(["katamari", "katamari damacy", "katamari ball"],
  R("Katamari", "Boulder", ["Attract", "Grow", "Sticky"], "Light", 2.0, 100.0, temp="Playful", inten=0.7), "Bandai Namco (Katamari Damacy)")
E(["fall guy", "fall guys", "fall guys bean"],
  R("Fall Guy", "Biped", ["Bounce"], "Light", 1.2, 20.0, 3, accent=PINK, temp="Playful", inten=1.0), "Mediatonic (Fall Guys)")
E(["untitled goose", "the goose", "honk", "untitled goose game"],
  R("Goose", "Bird", ["Grab", "Pull"], "Light", 0.9, 5.0, accent=WHITE, temp="Chaotic", inten=1.0), "House House (Untitled Goose Game)")
E(["goat simulator", "goat sim goat", "pilgor"],
  R("Goat", "Quadruped", ["Charge", "Pull"], "Light", 1.0, 60.0, temp="Chaotic", inten=1.0), "Coffee Stain (Goat Simulator)")
E(["meccha chameleon", "mecha chameleon"],
  R("Meccha Chameleon", "Biped", ["Sticky"], "Light", 1.2, 20.0, temp="Sneaky", inten=1.0), "Lemorion/Haganeiro (Meccha Chameleon)")

# ================================================================== ROCKET LEAGUE / SONIC / MEGA MAN
RL = "Psyonix (Rocket League)"
E(["octane", "rocket league octane", "rocket league car", "battle car", "rocket league"],
  R("Octane", "Vehicle", ["Fast", "Piloted", "Thrust"], "Fire", 1.2, 180.0, accent=ORANGE, feat=["Exhaust", "Wheels"]), RL)
E(["rocket league ball"], R("Rocket League Ball", "Ball", ["Bounce", "Heavy"], "Light", 2.0, 30.0), RL)
SN = "Sega (Sonic the Hedgehog)"
E(["spin dash", "sonic spin dash", "sonic ball", "sonic"],
  R("Spin Dash", "Ball", ["Charge", "Fast", "Spin"], "Wind", 1.0, 20.0, accent=BLUE, feat=["Spikes"]), SN)
E(["sonic spring", "spring", "yellow spring", "red spring"],
  R("Spring", "Trampoline", ["Bounce", "Launch"], "Light", 0.8, 5.0, accent=RED), SN)
E(["egg mobile", "eggmobile", "eggman's egg mobile", "wrecking ball egg mobile"],
  R("Egg Mobile", "Vehicle", ["Flies", "Piloted", "Shoots"], "Light", 2.0, 500.0,
    payload=R("checkered wrecking ball", "Boulder", ["Blunt", "Heavy"], "Light", 1.2, 300.0), feat=["Chains", "Cockpit"]), SN)
E(["chaos emerald", "chaos emeralds", "master emerald"],
  R("Chaos Emerald", "Ball", ["Fast", "Flies"], "Psychic", 0.4, 1.0, 7, accent=GREEN, temp="Protective", inten=0.8), SN)
E(["motobug", "badnik", "badniks"],
  R("Motobug", "Insect", ["Charge", "Walks"], "Light", 0.8, 20.0, feat=["Wheels"], temp="Aggressive", inten=0.6), SN)
MM = "Capcom (Mega Man)"
E(["mega buster", "megaman buster", "mega man buster", "x buster"],
  R("Mega Buster", "Cannon", ["Shoots"], "Light", 0.8, 5.0, accent=BLUE, payload=R("charged buster shot", "Ball", ["Fast", "Pierce"], "Light", 0.6, 1.0)), MM)
E(["rush", "rush coil", "rush jet", "megaman rush", "mega man rush"],
  R("Rush", "Quadruped", ["Bounce", "Launch"], "Light", 1.0, 40.0, accent=RED, temp="Protective", inten=0.8), MM)
E(["met", "metall", "mettaur", "hard hat enemy"],
  R("Metall", "Biped", ["Guard", "Shoots"], "Light", 0.5, 5.0, 4, accent=YELLOW, temp="Sneaky", inten=0.6), MM)

# ================================================================== CAPCOM / STREET FIGHTER / RESIDENT EVIL / FIGHTERS
SFT = "Capcom (Street Fighter)"
E(["hadouken", "hadoken", "hadoukens", "fireball hadouken"],
  R("Hadouken", "Ball", ["Blunt", "Fast"], "Light", 0.8, 3.0, accent=BLUE), SFT)
E(["shoryuken", "shoryu", "dragon punch"], R("Shoryuken", "Gauntlet", ["Launch"], "Fire", 1.0, 5.0), SFT)
E(["sonic boom", "guile sonic boom"], R("Sonic Boom", "Disc", ["Fast"], "Wind", 1.0, 1.0), SFT)
RE = "Capcom (Resident Evil)"
E(["mr. x", "mr x", "tyrant", "t-00", "tyrant t-103"],
  R("Mr. X", "Biped", ["Grab", "Heavy"], "Light", 2.6, 300.0, temp="Aggressive", inten=1.0), RE)
E(["nemesis", "stars", "re nemesis"],
  R("Nemesis", "Biped", ["Heavy", "Pull", "Shoots"], "Fire", 2.6, 400.0, payload=rocket(homing=False), temp="Aggressive", inten=1.0), RE)
E(["licker", "lickers"], R("Licker", "Quadruped", ["Pull", "Sticky"], "Poison", 1.2, 80.0, temp="Sneaky", inten=0.8), RE)
E(["dr. salvador", "chainsaw man re4", "chainsaw ganado"],
  R("Dr. Salvador", "Biped", ["Blade", "Spin"], "Light", 2.0, 100.0, feat=["Saw"], temp="Aggressive", inten=1.0), RE)
MK2 = "Warner Bros./NetherRealm (Mortal Kombat)"
E(["scorpion spear", "get over here", "kunai spear"],
  R("Scorpion's Spear", "Whip", ["Pierce", "Pull"], "Fire", 1.0, 2.0, feat=["Chains"]), MK2)
E(["sub-zero ice ball", "ice ball", "sub zero freeze"], R("Ice Ball", "Ball", ["Trap"], "Ice", 0.5, 1.0, accent=CYAN), MK2)
SSB = "Nintendo (Super Smash Bros.)"
E(["home-run bat", "home run bat", "homerun bat"],
  R("Home-Run Bat", "Hammer", ["Blunt", "Launch"], "Light", 1.0, 3.0), SSB)
E(["smash ball"], R("Smash Ball", "Ball", ["Explodes", "Flies"], "Light", 0.6, 1.0, accent=GOLD, temp="Playful", inten=1.0), SSB)
E(["franklin badge"], R("Franklin Badge", "Shield", ["Guard"], "Electric", 0.3, 0.2), SSB)
E(["final destination", "smash final destination", "fd stage"], R("Final Destination", "Platform", ["Barrier"], "Light", 8.0, 500.0, accent=PURPLE), SSB)

# ================================================================== MORE CLASSICS & 2025-26 HITS
E(["vampire killer", "belmont whip", "castlevania whip"],
  R("Vampire Killer", "Whip", ["Blade", "Pull"], "Light", 1.2, 2.0, feat=["Chains"]), "Konami (Castlevania)")
E(["holy water", "castlevania holy water"], R("Holy Water", "Bomb", ["Explodes"], "Fire", 0.3, 0.5), "Konami (Castlevania)")
E(["omniwrench", "omni wrench", "ratchet wrench"],
  R("OmniWrench", "Hammer", ["Blunt", "Returning"], "Light", 1.0, 5.0, accent=STEEL), "Insomniac (Ratchet & Clank)")
E(["sheepinator"], R("Sheepinator", "Gun", ["Shrink"], "Light", 0.8, 3.0, accent=WHITE), "Insomniac (Ratchet & Clank)")
E(["mr. zurkon", "mr zurkon", "zurkon"],
  R("Mr. Zurkon", "Blob", ["Flies", "Shoots"], "Electric", 0.5, 3.0, temp="Protective", inten=1.0), "Insomniac (Ratchet & Clank)")
E(["big daddy", "bouncer", "mr. bubbles"],
  R("Big Daddy", "Biped", ["Charge", "Heavy", "Spin"], "Light", 2.6, 900.0, feat=["Drill", "Armor"], temp="Protective", inten=1.0),
  "2K (BioShock)")
E(["plasma cutter"], R("Plasma Cutter", "Gun", ["Blade", "Pierce", "Shoots"], "Fire", 0.6, 2.0), "EA (Dead Space)")
E(["frostmourne"], R("Frostmourne", "Sword", ["Blade", "Heavy"], "Ice", 1.8, 10.0, accent=CYAN), "Blizzard (Warcraft)")
E(["zergling", "zerglings", "zerg rush"],
  R("Zergling", "Insect", ["Fast", "Swarm"], "Poison", 0.8, 20.0, 12, temp="Aggressive", inten=1.0), "Blizzard (StarCraft)")
E(["sentry gun", "tf2 sentry", "level 3 sentry", "engineer sentry"],
  R("Sentry Gun", "Pillar", ["Homing", "Shoots"], "Light", 1.2, 80.0, feat=["Barrels", "Turret"], payload=rocket()), "Valve (Team Fortress 2)")
E(["hidden blade", "assassin's blade", "assassins creed blade"],
  R("Hidden Blade", "Gauntlet", ["Blade", "Fast", "Pierce"], "Light", 0.4, 1.0), "Ubisoft (Assassin's Creed)")
E(["clicker", "clickers"], R("Clicker", "Biped", ["Grab"], "Poison", 1.8, 70.0, temp="Aggressive", inten=0.9), "Naughty Dog (The Last of Us)")
E(["claptrap", "cl4p-tp"], R("Claptrap", "Biped", ["Shoots"], "Electric", 1.2, 50.0, feat=["Wheels", "Antenna"], temp="Chaotic", inten=1.0), "Gearbox (Borderlands)")
E(["gaster blaster", "gaster blasters"],
  R("Gaster Blaster", "Blob", ["Beam", "Flies"], "Shadow", 1.5, 10.0, 2, temp="Fearful", inten=0.6), "Toby Fox (Undertale)")
E(["shovel blade", "shovel knight"], R("Shovel Blade", "Spear", ["Blunt", "Bounce"], "Earth", 1.2, 3.0, accent=BLUE), "Yacht Club (Shovel Knight)")
E(["coil-head", "coilhead", "coil head"],
  R("Coil-Head", "Biped", ["Bounce", "Fast"], "Light", 2.0, 60.0, temp="Sneaky", inten=1.0), "Zeekerss (Lethal Company)")
E(["hoarding bug", "yippee bug", "yippee"],
  R("Hoarding Bug", "Insect", ["Flies", "Grab"], "Light", 0.8, 10.0, temp="Playful", inten=0.8), "Zeekerss (Lethal Company)")
E(["leaper", "arc leaper"], R("Leaper", "Arachnid", ["Launch", "Stomp"], "Electric", 2.5, 1500.0, temp="Aggressive", inten=0.8), "Embark (ARC Raiders)")
E(["rocketeer", "arc rocketeer"],
  R("Rocketeer", "Vehicle", ["Flies", "Shoots"], "Fire", 2.5, 800.0, payload=rocket(), feat=["Rotor"], temp="Fearful", inten=0.6), "Embark (ARC Raiders)")
E(["bastion arc", "arc bastion"],
  R("ARC Bastion", "Quadruped", ["Heavy", "Shoots"], "Electric", 3.5, 5000.0, feat=["Armor", "Barrels"], temp="Lazy", inten=0.5), "Embark (ARC Raiders)")
E(["esquie"], R("Esquie", "Blob", ["Bounce", "Piloted"], "Water", 3.0, 800.0, temp="Playful", inten=0.8), "Sandfall (Clair Obscur: Expedition 33)")
E(["pikmin", "pikmins", "red pikmin", "blue pikmin", "yellow pikmin"],
  R("Pikmin", "Biped", ["Grab", "Swarm"], "Light", 0.4, 1.0, 20, temp="Protective", inten=0.7), "Nintendo (Pikmin)")
E(["bulborb", "bulborbs"], R("Bulborb", "Quadruped", ["Grab"], "Light", 1.6, 300.0, accent=RED, temp="Lazy", inten=0.7), "Nintendo (Pikmin)")
E(["mr. resetti", "mr resetti", "resetti"], R("Mr. Resetti", "Biped", ["Stomp"], "Earth", 1.0, 30.0, temp="Aggressive", inten=1.0), "Nintendo (Animal Crossing)")
E(["ban hammer", "banhammer", "roblox ban hammer"],
  R("Ban Hammer", "Hammer", ["Blunt", "DropFromSky", "Heavy"], "Light", 1.6, 80.0), "Roblox")
E(["just cause grapple", "rico grapple", "tether", "grappling hook tether", "retractor"],
  R("Tether", "Whip", ["Link", "Pull"], "Light", 1.2, 2.0, feat=["Chains"]), "Square Enix/Avalanche (Just Cause)")
E(["wingsuit", "wing suit"], R("Wingsuit", "Wings", ["Fast", "Glide"], "Wind", 1.8, 3.0), "Square Enix/Avalanche (Just Cause)")
E(["symbiote tendril", "venom tendril", "symbiote"],
  R("Symbiote Tendril", "Whip", ["Constrict", "Pull"], "Shadow", 1.5, 3.0, accent=BLACK), "Insomniac (Marvel's Spider-Man 2)")
E(["web wings"], R("Web Wings", "Wings", ["Fast", "Glide"], "Wind", 1.6, 1.0, accent=RED), "Insomniac (Marvel's Spider-Man 2)")
E(["owlbear", "owl bear", "bg3 owlbear", "baldur's gate owlbear"], R("Owlbear", "Quadruped", ["Charge", "Grab"], "Light", 2.4, 700.0, temp="Aggressive", inten=0.8), "Larian (Baldur's Gate 3)")
E(["mind flayer", "illithid", "bg3 mind flayer", "baldur's gate mind flayer"], R("Mind Flayer", "Biped", ["Constrict", "Grab"], "Psychic", 2.0, 80.0, accent=PURPLE, temp="Sneaky", inten=0.7), "Larian (Baldur's Gate 3)")
E(["nautiloid", "illithid ship"], R("Nautiloid", "Vehicle", ["Attract", "Flies", "Piloted"], "Psychic", 10.0, 50000.0, feat=["Spikes"]), "Larian (Baldur's Gate 3)")
E(["minsc and boo", "boo the hamster", "miniature giant space hamster"],
  R("Boo", "Quadruped", ["Fast", "Grow"], "Light", 0.3, 0.2, temp="Playful", inten=1.0), "Larian (Baldur's Gate 3)")

if __name__ == "__main__":
    n, ns = emit(sys.argv[1])
    emit_gallery(sys.argv[2])
    print(f"{n} entries, {ns} with shapes")
