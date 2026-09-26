"""Film/TV/comics icon pack generator (Brightest Day).

    python tools/icons/film_tv/gen.py          -> assets/icons/film_tv.ron
    python tools/icons/film_tv/names_gen.py    -> assets/icons/film_tv_names.txt (QIDs from qid_cache.json;
                                                  online Wikidata lookup only for titles not yet cached)
Names already owned by assets/icons.ron or another domain pack are dropped automatically (ftlib.emit).
Validate: cargo run --manifest-path tools/iconcheck/Cargo.toml -- assets/icons/film_tv.ron
"""
import os
import sys
from ftlib import E, R, emit, ENTRIES
import shapes_ft as S
import shapes_ft2 as S2

GOLD = (0.95, 0.75, 0.2)
SILVER = (0.82, 0.85, 0.9)
RED = (0.85, 0.12, 0.12)
BLUE = (0.15, 0.35, 0.85)
WOOD = (0.55, 0.38, 0.22)
WHITE = (0.95, 0.95, 0.95)
GREEN = (0.2, 0.9, 0.3)
YELLOW = (1.0, 0.85, 0.1)
ORANGE = (1.0, 0.55, 0.1)
PURPLE = (0.55, 0.25, 0.8)
PINK = (1.0, 0.45, 0.7)
BLACK = (0.12, 0.12, 0.14)
CYAN = (0.4, 0.9, 1.0)

GL = "DC (Green Lantern)"

# =============================================================== GREEN LANTERN (the premise)
E(["lantern fist", "green lantern fist", "giant green fist", "hal jordan's fist", "construct fist", "giant fist construct"],
  R("Lantern Fist", "Gauntlet", ["Blunt", "Heavy", "Launch"], scale=None, mass=120, accent=GREEN),
  GL, S.lantern_fist())
E(["boxing glove construct", "giant boxing glove", "guy gardner's boxing glove", "spring boxing glove", "boxing glove on a spring", "punching glove"],
  R("Boxing Glove Construct", "Gauntlet", ["Blunt", "Launch", "Thrust"], scale=None, mass=40, accent=RED),
  GL + " / Looney Tunes", S.boxing_glove())
E(["power battery", "green lantern power battery", "lantern battery", "green lantern lantern", "central power battery"],
  R("Power Battery", "Pillar", ["Barrier", "Repel"], scale=None, mass=400, accent=GREEN),
  GL, S.power_battery())
E(["green lantern chain", "lantern chains", "hal jordan's chain", "construct chain"],
  R("Lantern Chain", "Whip", ["Link", "Pull"], scale=4.5, mass=15, accent=GREEN), GL)
E(["hal jordan's jet", "ferris air jet", "green lantern jet", "hal's fighter jet"],
  R("Hal Jordan's Jet", "Vehicle", ["Fast", "Flies", "Piloted", "Shoots"], scale=6.0, mass=900, accent=GREEN,
    feat=["Cockpit", "Jet", "Fins"]), GL)
E(["john stewart's mech", "lantern mech", "john stewart mech", "green lantern mech", "john stewart's construct"],
  R("John Stewart's Mech", "Biped", ["Piloted", "Shoots", "Stomp"], scale=4.0, mass=2500, accent=GREEN,
    feat=["Armor", "Pistons", "Gears", "Cockpit"]), GL + " (Lanterns 2026)")
E(["kilowog", "kilowog's hammer", "poozer"],
  R("Kilowog", "Biped", ["Blunt", "Charge", "Heavy", "Stomp"], scale=3.4, mass=400, accent=GREEN,
    temp="Protective", inten=0.7), GL)
E(["guy gardner", "guy gardner's construct", "guy gardner's bat"],
  R("Guy Gardner's Construct", "Hammer", ["Blunt", "Charge", "Heavy"], scale=3.0, mass=80, accent=GREEN), GL)
E(["kyle rayner's construct", "kyle rayner"],
  R("Kyle Rayner's Construct", "Kaiju", ["Grab", "Piloted", "Stomp"], scale=12.0, mass=20000, accent=GREEN,
    feat=["Armor", "Cockpit"]), GL)
E(["parallax", "fear entity", "parallax entity"],
  R("Parallax", "Serpent", ["ArticulatedTail", "Constrict", "Flies"], el="Shadow", scale=10.0, mass=3000,
    accent=YELLOW, temp="Aggressive", inten=0.9), GL)
E(["sinestro", "sinestro's construct", "yellow lantern", "sinestro corps"],
  R("Sinestro's Construct", "Spear", ["Pierce", "Sticky", "Swarm"], el="Shadow", scale=3.0, mass=10, count=5,
    accent=YELLOW), GL)
E(["atrocitus", "red lantern", "red lantern corps", "napalm blood", "rage plasma"],
  R("Red Lantern Rage", "Biped", ["Breath", "Charge"], el="Fire", scale=3.0, mass=300, accent=RED,
    temp="Aggressive", inten=1.0), GL)
E(["dex-starr", "dexstarr", "red lantern cat", "rage kitty"],
  R("Dex-Starr", "Quadruped", ["Breath", "Fast"], el="Fire", scale=1.0, mass=6, accent=RED,
    temp="Aggressive", inten=0.95), GL)
E(["larfleeze", "orange lantern"],
  R("Larfleeze", "Quadruped", ["Grab", "Pull", "Swarm"], el="Poison", scale=2.2, mass=150, count=3,
    accent=ORANGE, temp="Chaotic", inten=0.8), GL)
E(["star sapphire", "star sapphire crystal", "violet lantern", "zamaron crystal"],
  R("Star Sapphire Crystal", "Cage", ["Pierce", "Trap"], el="Psychic", scale=3.0, mass=200, accent=PINK), GL)
E(["blue lantern", "blue lantern shield", "hope construct"],
  R("Blue Lantern Shield", "Dome", ["Barrier", "Bounce"], scale=5.0, mass=100, accent=BLUE), GL)
E(["indigo tribe staff", "indigo lantern", "indigo staff"],
  R("Indigo Tribe Staff", "Staff", ["Portal", "Beam"], el="Psychic", scale=3.2, mass=8, accent=PURPLE), GL)
E(["black lantern", "black lanterns", "black lantern corps", "nekron"],
  R("Black Lanterns", "Biped", ["Grab", "Swarm"], el="Shadow", scale=2.0, mass=80, count=4, accent=BLACK,
    temp="Chaotic", inten=0.9), GL)
E(["white lantern", "white lantern beam"],
  R("White Lantern", "Staff", ["Beam", "Repel"], scale=3.0, mass=5, accent=WHITE), GL)
E(["mogo", "mogo the living planet", "living planet"],
  R("Mogo", "Ball", ["Attract", "DropFromSky", "Heavy"], el="Earth", scale=30.0, mass=100000, accent=GREEN), GL)
E(["ch'p", "chp", "b'dg", "squirrel lantern"],
  R("Ch'p", "Quadruped", ["Fast", "Swarm"], scale=0.8, mass=2, count=3, accent=GREEN, temp="Playful", inten=0.8), GL)
E(["lantern jet cage", "green lantern cage", "lantern prison"],
  R("Lantern Cage", "Cage", ["DropFromSky", "Trap"], scale=3.0, mass=150, accent=GREEN), GL)
E(["lantern gatling", "green lantern minigun", "construct minigun"],
  R("Lantern Minigun", "Gun", ["Fast", "Shoots"], scale=2.2, mass=30, accent=GREEN, feat=["Barrels"]), GL)

# =============================================================== MARVEL
MV = "Marvel"
E(["mjolnir", "thor's hammer", "mjölnir", "mjolner", "mjolnor", "thors hammer"],
  R("Mjolnir", "Hammer", ["Blunt", "Returning"], el="Electric", scale=None, mass=60, accent=SILVER),
  MV + " (Thor)", S.mjolnir())
E(["stormbreaker", "storm breaker", "thor's axe", "thors axe"],
  R("Stormbreaker", "Axe", ["Blade", "Returning"], el="Electric", scale=None, mass=50, accent=WOOD),
  MV + " (Thor)", S.stormbreaker())
E(["captain america's shield", "cap's shield", "captain america shield", "caps shield", "vibranium shield", "cap shield"],
  R("Captain America's Shield", "Shield", ["Guard", "Returning", "Ricochet"], scale=None, mass=6, accent=WHITE),
  MV + " (Captain America)", S.cap_shield())
E(["iron man suit", "iron man armor", "iron man", "ironman", "mark 85", "iron man armour"],
  R("Iron Man Suit", "Biped", ["Beam", "Flies", "Piloted", "Thrust"], scale=None, mass=200, accent=GOLD,
    feat=["Armor", "Jet"]), MV + " (Iron Man)", S.iron_man())
E(["hulkbuster", "hulk buster", "veronica armor"],
  R("Hulkbuster", "Biped", ["Blunt", "Heavy", "Piloted", "Stomp"], scale=4.0, mass=3000, accent=GOLD,
    feat=["Armor", "Jet", "Pistons"]), MV + " (Avengers)")
E(["ant-man suit", "antman suit", "ant man", "ant-man", "pym particles", "pym particle disc"],
  R("Pym Particle Disc", "Disc", ["Grow", "Shrink"], scale=1.2, mass=1, accent=RED), MV + " (Ant-Man)")
E(["giant-man", "giant man", "giant ant-man"],
  R("Giant-Man", "Kaiju", ["Grab", "Stomp"], scale=16.0, mass=30000, accent=RED), MV + " (Ant-Man)")
E(["sling ring portal", "doctor strange portal", "sling ring", "dr strange portal", "strange portal", "sparkly portal"],
  R("Sling Ring Portal", "Platform", ["Portal"], scale=None, mass=5, accent=ORANGE),
  MV + " (Doctor Strange)", S.sling_ring_portal())
E(["cloak of levitation", "doctor strange cloak", "dr strange cape"],
  R("Cloak of Levitation", "Cape", ["Glide", "Grab", "SlowFall"], scale=1.8, mass=3, accent=RED), MV + " (Doctor Strange)")
E(["web shooters", "web shooter", "spider-man web shooter", "spiderman web", "web slinger", "spider web shooter"],
  R("Web Shooters", "Gun", ["Pull", "Shoots", "Sticky"], scale=None, mass=1, accent=RED),
  MV + " (Spider-Man: Brand New Day)", S.web_shooter())
E(["iron spider", "iron spider suit", "spider arms", "waldoes"],
  R("Iron Spider", "Arachnid", ["Grab", "Piloted", "Sticky"], scale=2.4, mass=120, accent=GOLD,
    feat=["Armor"]), MV + " (Spider-Man)")
E(["spider-man", "spiderman", "spider man", "spidey", "miles morales", "spider-verse"],
  R("Spider-Man", "Biped", ["Fast", "Pull", "Sticky"], scale=2.0, mass=75, accent=RED,
    temp="Playful", inten=0.7), MV + " (Spider-Man / Spider-Verse)")
E(["wolverine claws", "adamantium claws", "wolverine's claws", "logan's claws"],
  R("Wolverine's Claws", "Gauntlet", ["Blade", "Fast", "Pierce"], scale=None, mass=12, accent=SILVER),
  MV + " (X-Men)", S.wolverine_claws())
E(["infinity gauntlet", "thanos gauntlet", "infinity stones", "thanos glove"],
  R("Infinity Gauntlet", "Gauntlet", ["Attract", "Beam", "Blunt"], el="Psychic", scale=None, mass=30, accent=GOLD),
  MV + " (Avengers)", S.infinity_gauntlet())
E(["vibranium claws", "black panther claws", "wakandan claws", "panther habit"],
  R("Black Panther's Claws", "Gauntlet", ["Blade", "Fast", "Repel"], scale=1.8, mass=6, accent=PURPLE),
  MV + " (Black Panther)")
E(["groot", "baby groot", "flora colossus"],
  R("Groot", "Biped", ["Constrict", "Grab"], el="Earth", scale=None, mass=250, accent=WOOD,
    temp="Protective", inten=0.8), MV + " (Guardians of the Galaxy)", S.groot())
E(["hulk", "the hulk", "incredible hulk", "red hulk"],
  R("Hulk", "Biped", ["Blunt", "Charge", "Heavy", "Stomp"], scale=3.6, mass=600, accent=GREEN,
    temp="Aggressive", inten=0.9), MV + " (Hulk)")
E(["galactus", "devourer of worlds", "world eater"],
  R("Galactus", "Kaiju", ["Attract", "Beam", "Stomp"], el="Atomic", scale=22.0, mass=100000, accent=PURPLE,
    feat=["Armor"]), MV + " (The Fantastic Four: First Steps)")
E(["silver surfer board", "silver surfer", "surfer board", "cosmic surfboard", "silver surfer's board"],
  R("Silver Surfer's Board", "Vehicle", ["Fast", "Flies", "Piloted"], scale=None, mass=50, accent=SILVER),
  MV + " (Fantastic Four)", S.surfboard())
E(["the thing", "ben grimm", "thing fantastic four"],
  R("The Thing", "Biped", ["Blunt", "Heavy", "Stomp"], el="Earth", scale=2.6, mass=500, accent=ORANGE),
  MV + " (Fantastic Four)")
E(["human torch", "johnny storm"],
  R("Human Torch", "Bird", ["Breath", "Fast", "Flies"], el="Fire", scale=1.8, mass=70, accent=ORANGE,
    temp="Playful", inten=0.6), MV + " (Fantastic Four)")
E(["symbiote", "venom symbiote", "knull"],
  R("Venom", "Blob", ["Constrict", "Grab", "Sticky"], el="Shadow", scale=2.6, mass=250, accent=BLACK,
    temp="Aggressive", inten=0.85), MV + " (Venom)")
E(["x-men sentinel", "mutant hunter robot"],
  R("Sentinel", "Kaiju", ["Beam", "Flies", "Grab"], scale=14.0, mass=50000, accent=PURPLE,
    feat=["Armor", "Jet"], temp="Aggressive", inten=0.7), MV + " (X-Men)")
E(["optic blast", "cyclops visor", "cyclops beam"],
  R("Optic Blast", "Gun", ["Beam", "Repel"], scale=1.2, mass=1, accent=RED), MV + " (X-Men)")
E(["hawkeye's bow", "hawkeye bow", "trick arrow", "hawkeye", "explosive arrow"],
  R("Hawkeye's Trick Arrows", "Bow", ["Explodes", "Sticky"], scale=2.2, mass=3, accent=PURPLE), MV + " (Hawkeye)")
E(["loki's scepter", "loki scepter", "mind stone scepter", "loki"],
  R("Loki's Scepter", "Staff", ["Beam", "Pierce"], el="Psychic", scale=2.8, mass=5, accent=GOLD), MV + " (Avengers)")
E(["deadpool's katanas", "deadpool katanas", "deadpool", "deadpool swords"],
  R("Deadpool's Katanas", "Sword", ["Blade", "Fast"], scale=2.2, mass=3, count=2, accent=RED), MV + " (Deadpool)")
E(["hellfire chain", "ghost rider chain", "ghost rider", "penance chain"],
  R("Ghost Rider's Chain", "Whip", ["Link", "Pull"], el="Fire", scale=4.5, mass=15, accent=ORANGE), MV + " (Ghost Rider)")
E(["hell cycle", "ghost rider bike", "ghost rider motorcycle", "hellcycle"],
  R("Hell Cycle", "Vehicle", ["Charge", "Fast", "Piloted"], el="Fire", scale=3.0, mass=300, accent=ORANGE,
    feat=["Wheels", "Exhaust", "Spikes"]), MV + " (Ghost Rider)")
E(["daredevil's billy club", "daredevil", "daredevil baton"],
  R("Daredevil's Billy Club", "Staff", ["Pull", "Returning", "Ricochet"], scale=1.6, mass=2, accent=RED),
  MV + " (Daredevil: Born Again)")
E(["quinjet", "avengers quinjet", "avengers jet"],
  R("Quinjet", "Vehicle", ["Flies", "Piloted", "Shoots"], scale=9.0, mass=8000, accent=SILVER,
    feat=["Cockpit", "Jet", "Turret"]), MV + " (Avengers)")
E(["helicarrier", "shield helicarrier", "s.h.i.e.l.d. helicarrier"],
  R("Helicarrier", "Platform", ["Flies", "Shoots"], scale=30.0, mass=90000, accent=SILVER,
    feat=["Rotor", "Turret", "Windows"]), MV + " (Avengers)")
E(["thanos' sword", "thanos sword", "thanos double sword", "thanos"],
  R("Thanos' Double-Edged Sword", "Staff", ["Blade", "Heavy", "Spin"], scale=3.6, mass=60, accent=GOLD),
  MV + " (Avengers: Endgame)")
E(["repulsor", "repulsor blast", "iron man glove", "iron man gauntlet"],
  R("Repulsor Gauntlet", "Gauntlet", ["Beam", "Repel", "Thrust"], scale=1.8, mass=10, accent=GOLD), MV + " (Iron Man)")
E(["war machine", "war machine minigun", "rhodey"],
  R("War Machine", "Biped", ["Flies", "Piloted", "Shoots"], scale=2.6, mass=300, accent=SILVER,
    feat=["Armor", "Barrels", "Jet"]), MV + " (Iron Man)")

# =============================================================== DC
DC = "DC"
E(["batarang", "bat-a-rang", "batman's batarang", "bat boomerang", "batarangs"],
  R("Batarang", "Disc", ["Blade", "Returning", "Ricochet"], scale=None, mass=0.5, accent=BLACK),
  DC + " (Batman)", S.batarang())
E(["batmobile", "the batmobile", "batman's car", "bat mobile", "batman car"],
  R("Batmobile", "Vehicle", ["Charge", "Heavy", "Piloted", "Shoots"], scale=None, mass=3000, accent=BLACK,
    feat=["Wheels", "Jet", "Armor", "Cockpit"]), DC + " (Batman)", S.batmobile())
E(["batwing", "the batwing", "batman's jet", "bat plane"],
  R("Batwing", "Vehicle", ["Flies", "Piloted", "Shoots"], scale=7.0, mass=4000, accent=BLACK,
    feat=["Cockpit", "Jet", "Fins"]), DC + " (Batman)")
E(["bat-signal", "bat signal", "batsignal"],
  R("Bat-Signal", "Pillar", ["Beam", "Barrier"], scale=None, mass=300, accent=YELLOW, feat=["Headlight"]),
  DC + " (Batman)", S2.bat_signal())
E(["grappling gun", "batclaw", "batman grapple", "grapnel gun", "bat grapple"],
  R("Batman's Grapnel Gun", "Gun", ["Pull"], scale=1.4, mass=2, accent=BLACK), DC + " (Batman)")
E(["lasso of truth", "wonder woman's lasso", "golden lasso", "lasso of hestia", "wonder woman lasso", "lasso of truth wonder woman"],
  R("Lasso of Truth", "Whip", ["Constrict", "Link", "Pull"], scale=5.0, mass=2, accent=GOLD),
  DC + " (Wonder Woman)")
E(["wonder woman's bracelets", "bracelets of submission", "wonder woman bracers", "wonder woman"],
  R("Bracelets of Submission", "Gauntlet", ["Guard", "Ricochet", "Repel"], scale=1.6, mass=4, accent=SILVER),
  DC + " (Wonder Woman)")
E(["aquaman's trident", "trident of atlan", "aquaman trident", "aquaman"],
  R("Trident of Atlan", "Spear", ["Pierce", "Returning"], el="Water", scale=None, mass=15, accent=GOLD),
  DC + " (Aquaman)", S.trident())
E(["superman", "man of steel", "clark kent", "kal-el", "supes"],
  R("Superman", "Biped", ["Beam", "Fast", "Flies"], el="Fire", scale=2.2, mass=110, accent=RED,
    temp="Protective", inten=0.8), DC + " (Superman 2025)")
E(["krypto", "krypto the superdog", "superdog", "super dog", "superman's dog"],
  R("Krypto", "Quadruped", ["Charge", "Fast", "Flies"], scale=None, mass=40, accent=RED,
    temp="Playful", inten=0.9), DC + " (Superman 2025 / Supergirl)", S.krypto())
E(["supergirl", "kara zor-el"],
  R("Supergirl", "Biped", ["Beam", "Flies", "Charge"], el="Fire", scale=2.0, mass=60, accent=RED,
    temp="Aggressive", inten=0.6), DC + " (Supergirl 2026)")
E(["kryptonite", "green kryptonite", "kryptonite rock"],
  R("Kryptonite", "Boulder", ["Heavy"], el="Poison", scale=None, mass=200, accent=GREEN), DC + " (Superman)", S2.kryptonite())
E(["kryptonite spear", "doomsday spear"],
  R("Kryptonite Spear", "Spear", ["Pierce"], el="Poison", scale=3.2, mass=10, accent=GREEN), DC + " (Batman v Superman)")
E(["t-spheres", "mister terrific's t-spheres", "t spheres", "mr terrific t-spheres", "tspheres"],
  R("T-Spheres", "Ball", ["Flies", "Homing", "Shoots", "Swarm"], el="Electric", scale=0.6, mass=1, count=6,
    accent=SILVER, temp="Protective", inten=0.6), DC + " (Superman 2025)")
E(["boxing glove arrow", "green arrow boxing glove", "green arrow", "trick arrows green arrow"],
  R("Boxing Glove Arrow", "Bow", ["Blunt", "Launch"], scale=2.2, mass=3, accent=GREEN), DC + " (Green Arrow)")
E(["harley quinn's mallet", "harley's mallet", "harley quinn mallet", "harley quinn", "harley hammer"],
  R("Harley Quinn's Mallet", "Hammer", ["Blunt", "Launch"], scale=None, mass=25, accent=RED),
  DC + " (Harley Quinn / Suicide Squad)", S.harley_mallet())
E(["penguin's umbrella", "penguin umbrella", "oz cobb umbrella"],
  R("Penguin's Umbrella", "Staff", ["Shoots", "SlowFall"], scale=2.4, mass=3, accent=PURPLE),
  DC + " (Batman / The Penguin)")
E(["freeze gun", "mr freeze gun", "mr. freeze", "mister freeze", "cryo gun"],
  R("Mr. Freeze's Freeze Gun", "Gun", ["Beam"], el="Ice", scale=2.0, mass=15, accent=CYAN), DC + " (Batman)")
E(["poison ivy", "ivy's vines", "poison ivy vines", "pamela isley"],
  R("Poison Ivy's Vines", "Serpent", ["Constrict", "Pull", "Swarm"], el="Poison", scale=4.0, mass=40, count=3,
    accent=GREEN, temp="Sneaky", inten=0.7), DC + " (Batman)")
E(["omega beams", "darkseid", "darkseid's omega beams", "omega effect"],
  R("Omega Beams", "Gun", ["Homing", "Shoots"], el="Fire", scale=1.6, mass=5, accent=RED), DC + " (Darkseid)")
E(["boom tube", "new gods boom tube", "mother box"],
  R("Boom Tube", "Platform", ["Portal", "Explodes"], el="Electric", scale=5.0, mass=5, accent=CYAN), DC + " (New Gods)")
E(["the flash", "barry allen", "speed force", "wally west"],
  R("The Flash", "Biped", ["Charge", "Fast"], el="Electric", scale=2.0, mass=80, accent=RED,
    temp="Playful", inten=0.7), DC + " (The Flash)")
E(["sonic cannon", "cyborg's sonic cannon", "cyborg cannon"],
  R("Cyborg's Sonic Cannon", "Cannon", ["Beam", "Repel"], scale=2.2, mass=20, accent=SILVER), DC + " (Justice League)")
E(["nth metal mace", "hawkgirl's mace", "hawkgirl mace", "hawkgirl", "hawkman mace"],
  R("Nth Metal Mace", "Hammer", ["Blunt", "Glide"], el="Electric", scale=2.4, mass=20, accent=GOLD), DC + " (Hawkgirl)")
E(["starro", "starro the conqueror", "space starfish"],
  R("Starro", "Kaiju", ["Stomp", "Swarm", "Sticky"], el="Psychic", scale=16.0, mass=40000, count=1, accent=BLUE,
    payload=R("Starro Spore", "Blob", ["Flies", "Grab", "Sticky"], el="Psychic", scale=0.8, mass=2, count=4)),
  DC + " (The Suicide Squad / Peacemaker)")
E(["king shark", "nanaue"],
  R("King Shark", "Biped", ["Charge", "Grab"], el="Water", scale=3.0, mass=500, accent=BLUE,
    temp="Playful", inten=0.8), DC + " (Suicide Squad)")
E(["peacemaker's helmet", "peacemaker helmet"],
  R("Peacemaker's Helmet", "Gauntlet", ["Blunt", "Repel"], scale=1.4, mass=4, accent=SILVER), DC + " (Peacemaker)")
E(["doomsday monster", "doomsday dc"],
  R("Doomsday", "Biped", ["Charge", "Heavy", "Stomp"], el="Earth", scale=3.6, mass=900, accent=SILVER,
    feat=["Spikes"], temp="Aggressive", inten=1.0), DC + " (Superman)")
E(["joker gas", "joker venom", "laughing gas", "joker's laughing gas", "the joker"],
  R("Joker Venom", "Bomb", ["Explodes"], el="Poison", scale=1.2, mass=2, accent=GREEN), DC + " (Batman)")

# =============================================================== STAR WARS
SW = "Star Wars"
E(["lightsaber", "light saber", "lightsabre", "laser sword", "jedi sword", "lite saber", "sith lightsaber", "green lightsaber"],
  R("Lightsaber", "Sword", ["Blade", "Fast"], scale=None, mass=1.5, accent=SILVER), SW, S.lightsaber())
E(["darksaber", "dark saber", "mandalorian darksaber"],
  R("Darksaber", "Sword", ["Blade", "Heavy"], el="Shadow", scale=None, mass=2, accent=BLACK),
  SW + " (The Mandalorian)", S.darksaber())
E(["double-bladed lightsaber", "double lightsaber", "darth maul lightsaber", "saberstaff", "double bladed lightsaber", "darth maul"],
  R("Double-Bladed Lightsaber", "Staff", ["Blade", "Spin"], scale=None, mass=2, accent=RED),
  SW + " (The Phantom Menace)", S.double_saber())
E(["death star", "the death star", "deathstar", "death star laser", "superlaser"],
  R("Death Star", "Ball", ["Beam", "DropFromSky", "Explodes"], el="Atomic", scale=None, mass=100000,
    accent=SILVER), SW, S.death_star())
E(["at-at", "at at", "atat", "imperial walker", "at-at walker", "all terrain armored transport"],
  R("AT-AT", "Kaiju", ["Heavy", "Piloted", "Shoots", "Stomp"], scale=None, mass=60000, accent=SILVER,
    feat=["Armor", "Cockpit", "Barrels"]), SW + " (The Empire Strikes Back)", S.at_at())
E(["x-wing", "x wing", "xwing", "t-65 x-wing", "x-wing fighter"],
  R("X-Wing", "Vehicle", ["Fast", "Flies", "Piloted", "Shoots"], scale=None, mass=10000, accent=RED,
    feat=["Cockpit", "Jet"]), SW, S.x_wing())
E(["tie fighter", "tie-fighter", "tie fighters", "twin ion engine"],
  R("TIE Fighter", "Vehicle", ["Fast", "Flies", "Piloted", "Shoots"], scale=None, mass=5000, accent=BLACK,
    feat=["Cockpit", "Windows"]), SW, S.tie_fighter())
E(["millennium falcon", "millenium falcon", "han solo's ship", "millennium falcon ship"],
  R("Millennium Falcon", "Vehicle", ["Fast", "Flies", "Piloted", "Shoots"], scale=None, mass=100000, accent=SILVER,
    feat=["Cockpit", "Dish", "Turret", "Jet"]), SW, S.millennium_falcon())
E(["grogu", "baby yoda", "grogu the child", "yoda baby", "din grogu"],
  R("Grogu", "Biped", ["Grab", "Repel"], el="Psychic", scale=None, mass=8, accent=WOOD,
    temp="Protective", inten=0.7), SW + " (The Mandalorian and Grogu 2026)", S.grogu())
E(["yoda", "master yoda"],
  R("Yoda", "Biped", ["Fast", "Repel", "Spin"], el="Psychic", scale=1.2, mass=15, accent=GREEN,
    temp="Playful", inten=0.6), SW)
E(["mandalorian jetpack", "mando jetpack", "boba fett jetpack", "beskar jetpack", "jetpack mandalorian"],
  R("Mandalorian Jetpack", "Jetpack", ["Homing", "Shoots", "Thrust"], scale=1.2, mass=12, accent=SILVER,
    feat=["Jet"]), SW + " (The Mandalorian)")
E(["whistling birds", "whistling bird", "mandalorian whistling birds"],
  R("Whistling Birds", "Rocket", ["Explodes", "Homing", "Swarm"], scale=0.6, mass=1, count=8, accent=SILVER),
  SW + " (The Mandalorian)")
E(["beskar spear", "the mandalorian spear", "beskar staff"],
  R("Beskar Spear", "Spear", ["Guard", "Pierce"], scale=3.2, mass=8, accent=SILVER), SW + " (The Mandalorian)")
E(["thermal detonator", "thermal detonators", "star wars grenade"],
  R("Thermal Detonator", "Bomb", ["Explodes"], el="Fire", scale=1.0, mass=2, accent=SILVER), SW)
E(["force lightning", "sith lightning", "palpatine lightning", "emperor palpatine"],
  R("Force Lightning", "Gauntlet", ["Beam"], el="Electric", scale=1.4, mass=1, accent=PURPLE), SW)
E(["force push", "force blast", "jedi push"],
  R("Force Push", "Gauntlet", ["Repel"], el="Psychic", scale=1.4, mass=1, accent=CYAN), SW)
E(["force grip", "force pull", "force choke", "darth vader", "vader"],
  R("Force Grip", "Gauntlet", ["Grab", "Pull"], el="Psychic", scale=1.4, mass=1, accent=RED), SW)
E(["bb-8", "bb8", "bb 8", "beebee-ate"],
  R("BB-8", "Ball", ["Bounce", "Charge", "Fast"], scale=None, mass=18, accent=ORANGE, temp="Playful", inten=0.8), SW, S2.bb_8())
E(["r2-d2", "r2d2", "artoo", "r2 d2"],
  R("R2-D2", "Vehicle", ["Charge", "Homing"], el="Electric", scale=None, mass=32, accent=BLUE,
    feat=["Wheels", "Antenna", "Dish"], temp="Playful", inten=0.7), SW, S2.r2_d2())
E(["sarlacc", "sarlacc pit", "the sarlacc"],
  R("Sarlacc Pit", "Cage", ["Attract", "Grab", "Trap"], el="Earth", scale=5.0, mass=5000, accent=ORANGE,
    feat=["Spikes"]), SW + " (Return of the Jedi)")
E(["rancor monster"],
  R("Rancor", "Kaiju", ["Grab", "Stomp"], scale=10.0, mass=6000, accent=WOOD, temp="Aggressive", inten=0.8), SW)
E(["speeder bike", "74-z speeder bike", "endor speeder"],
  R("Speeder Bike", "Vehicle", ["Fast", "Flies", "Piloted"], scale=3.5, mass=200, accent=SILVER,
    feat=["Jet", "Fins"]), SW + " (Return of the Jedi)")
E(["star destroyer", "imperial star destroyer"],
  R("Star Destroyer", "Vehicle", ["DropFromSky", "Heavy", "Shoots"], scale=None, mass=100000, accent=SILVER,
    feat=["Turret", "Jet", "Windows"]), SW, S2.star_destroyer())
E(["at-st", "at st", "chicken walker", "scout walker"],
  R("AT-ST", "Biped", ["Piloted", "Shoots", "Stomp"], scale=None, mass=5000, accent=SILVER,
    feat=["Armor", "Cockpit", "Barrels"]), SW, S2.at_st())
E(["porg", "porgs"],
  R("Porgs", "Bird", ["Flies", "Swarm"], scale=0.5, mass=0.5, count=8, accent=ORANGE, temp="Playful", inten=0.9),
  SW + " (The Last Jedi)")
E(["tauntaun"],
  R("Tauntaun", "Biped", ["Charge", "Piloted"], el="Ice", scale=2.6, mass=300, accent=WHITE), SW)
E(["wookiee bowcaster", "bowcaster", "chewbacca's bowcaster", "chewbacca", "chewie"],
  R("Bowcaster", "Bow", ["Explodes", "Shoots"], scale=2.0, mass=8, accent=WOOD), SW)

# =============================================================== LORD OF THE RINGS / HOBBIT
LOTR = "The Lord of the Rings"
E(["sting sword", "bilbo's sword", "frodo's sword", "elven dagger", "sting lotr"],
  R("Sting", "Sword", ["Blade", "Fast"], scale=None, mass=1.5, accent=CYAN), LOTR, S.sting())
E(["anduril", "andúril", "narsil", "flame of the west", "aragorn's sword"],
  R("Andúril", "Sword", ["Blade", "Heavy"], scale=2.8, mass=4, accent=SILVER), LOTR)
E(["the one ring", "one ring", "ring of power", "sauron's ring"],
  R("The One Ring", "Disc", ["Attract", "Returning"], el="Shadow", scale=None, mass=1, accent=GOLD), LOTR, S2.one_ring())
E(["smaug", "smaug the golden", "smaug the terrible"],
  R("Smaug", "Kaiju", ["ArticulatedTail", "Breath", "Flies", "Stomp"], el="Fire", scale=16.0, mass=60000,
    accent=GOLD, temp="Aggressive", inten=0.8), "The Hobbit", S.smaug())
E(["balrog", "durin's bane", "balrog of morgoth"],
  R("Balrog", "Kaiju", ["Breath", "Pull", "Stomp"], el="Fire", scale=12.0, mass=30000, accent=ORANGE,
    feat=["Spikes"], temp="Aggressive", inten=0.9), LOTR)
E(["gandalf's staff", "gandalf staff", "gandalf", "wizard staff of gandalf", "white wizard staff"],
  R("Gandalf's Staff", "Staff", ["Beam", "Repel"], scale=3.0, mass=4, accent=WHITE), LOTR)
E(["oliphaunt", "mumakil", "mûmakil", "mumak"],
  R("Oliphaunt", "Kaiju", ["Charge", "Heavy", "Piloted", "Stomp"], el="Earth", scale=12.0, mass=40000, accent=WOOD,
    feat=["Spikes"]), LOTR)
E(["ent", "ents", "treebeard", "fangorn"],
  R("Ent", "Biped", ["Grab", "Heavy", "Stomp"], el="Earth", scale=4.0, mass=2000, accent=WOOD,
    temp="Lazy", inten=0.6), LOTR)
E(["fell beast", "nazgul", "nazgûl", "ringwraith", "witch-king"],
  R("Fell Beast", "Bird", ["Flies", "Grab", "Piloted"], el="Shadow", scale=4.0, mass=900, accent=BLACK,
    temp="Aggressive", inten=0.7), LOTR)
E(["shelob", "giant spider shelob", "shelob's lair"],
  R("Shelob", "Arachnid", ["Grab", "Sticky"], el="Poison", scale=4.0, mass=800, accent=BLACK,
    temp="Sneaky", inten=0.8), LOTR)
E(["great eagle", "eagles of manwe", "gwaihir"],
  R("Great Eagle", "Bird", ["Flies", "Grab", "Piloted"], el="Wind", scale=4.0, mass=400, accent=WOOD,
    temp="Protective", inten=0.6), LOTR)
E(["grond", "hammer of the underworld", "grond battering ram"],
  R("Grond", "Vehicle", ["Charge", "Heavy"], el="Fire", scale=10.0, mass=20000, accent=BLACK,
    feat=["Wheels", "Chains", "Spikes"]), LOTR)

# =============================================================== HARRY POTTER / WIZARDING
HP = "Harry Potter"
E(["elder wand", "the elder wand", "deathstick", "wand of destiny", "harry potter wand", "magic wand of dumbledore"],
  R("Elder Wand", "Staff", ["Beam", "Shoots"], scale=None, mass=0.5, accent=WOOD), HP, S.elder_wand())
E(["nimbus 2000", "nimbus two thousand", "quidditch broom", "harry's broomstick", "nimbus"],
  R("Nimbus 2000", "Vehicle", ["Fast", "Flies", "Piloted"], scale=None, mass=5, accent=WOOD), HP, S.broom())
E(["expecto patronum", "patronus", "stag patronus", "patronus charm"],
  R("Patronus", "Quadruped", ["Charge", "Flies", "Repel"], scale=2.4, mass=5, accent=WHITE,
    temp="Protective", inten=0.9), HP)
E(["golden snitch", "the snitch"],
  R("Golden Snitch", "Ball", ["Fast", "Flies"], scale=0.4, mass=0.1, accent=GOLD, temp="Fearful", inten=0.9), HP)
E(["bludger", "bludgers"],
  R("Bludger", "Ball", ["Blunt", "Homing"], scale=0.8, mass=5, accent=BLACK, temp="Aggressive", inten=0.9), HP)
E(["basilisk", "chamber of secrets snake", "basilisk of slytherin"],
  R("Basilisk", "Serpent", ["Charge", "Constrict"], el="Poison", scale=12.0, mass=3000, accent=GREEN,
    temp="Sneaky", inten=0.7), HP)
E(["whomping willow", "the whomping willow"],
  R("Whomping Willow", "Pillar", ["Blunt", "Repel", "Spin"], el="Earth", scale=8.0, mass=5000, accent=WOOD), HP)
E(["dementor", "dementors"],
  R("Dementors", "Bird", ["Flies", "Swarm"], el="Shadow", scale=2.4, mass=20, count=4, accent=BLACK,
    temp="Aggressive", inten=0.8), HP)
E(["hogwarts express", "the hogwarts express"],
  R("Hogwarts Express", "Vehicle", ["Charge", "Heavy"], scale=None, mass=80000, accent=RED,
    feat=["Wheels", "Boiler", "Smokestack", "Pistons", "Cab"]), HP, S2.hogwarts_express())
E(["mandrake", "mandrake root", "screaming mandrake"],
  R("Mandrake", "Blob", ["Repel"], el="Wind", scale=0.6, mass=1, accent=GREEN, temp="Chaotic", inten=0.8), HP)
E(["hungarian horntail", "horntail", "triwizard dragon"],
  R("Hungarian Horntail", "Kaiju", ["ArticulatedTail", "Breath", "Flies"], el="Fire", scale=11.0, mass=15000,
    accent=BLACK, feat=["Spikes"], temp="Aggressive", inten=0.9), HP)
E(["fawkes", "dumbledore's phoenix"],
  R("Fawkes", "Bird", ["Flies", "Glide"], el="Fire", scale=1.8, mass=5, accent=RED, temp="Protective", inten=0.8), HP)

# =============================================================== MONSTERVERSE / KAIJU / DINOS
MVS = "Godzilla (Toho / Monsterverse)"
E(["godzilla", "gojira", "king of the monsters", "godzila", "godzilla minus one", "shin godzilla"],
  R("Godzilla", "Kaiju", ["ArticulatedTail", "Breath", "Stomp"], el="Atomic", scale=None, mass=90000,
    accent=CYAN, temp="Aggressive", inten=0.6), MVS, S.godzilla())
E(["king kong", "kong", "kong the eighth wonder"],
  R("King Kong", "Kaiju", ["Blunt", "Charge", "Grab", "Stomp"], scale=None, mass=70000, accent=WOOD,
    temp="Protective", inten=0.6), "King Kong (Monsterverse)", S.king_kong())
E(["kong's axe", "kong axe", "godzilla axe", "dorsal plate axe"],
  R("Kong's Axe", "Axe", ["Blade", "Heavy"], el="Atomic", scale=None, mass=200, accent=CYAN),
  "Godzilla vs. Kong", S.kong_axe())
E(["mothra", "queen of the monsters", "mosura"],
  R("Mothra", "Kaiju", ["Breath", "Flies", "Repel"], el="Wind", scale=12.0, mass=15000, accent=CYAN,
    temp="Protective", inten=0.9), MVS, S.mothra())
E(["king ghidorah", "ghidorah", "monster zero"],
  R("King Ghidorah", "Kaiju", ["ArticulatedTail", "Beam", "Flies", "Stomp"], el="Electric", scale=None,
    mass=90000, accent=GOLD, temp="Chaotic", inten=0.9), MVS, S.ghidorah())
E(["rodan", "radon kaiju", "fire demon rodan"],
  R("Rodan", "Bird", ["Charge", "Flies"], el="Fire", scale=4.0, mass=5000, accent=ORANGE,
    temp="Aggressive", inten=0.8), MVS)
E(["mechagodzilla", "kiryu", "mecha godzilla"],
  R("Mechagodzilla", "Kaiju", ["ArticulatedTail", "Beam", "Piloted", "Shoots"], el="Electric", scale=18.0,
    mass=90000, accent=SILVER, feat=["Armor", "Jet", "Barrels"]), MVS)
E(["gipsy danger", "jaeger", "gipsy avenger", "pacific rim robot", "jaegar", "striker eureka"],
  R("Gipsy Danger", "Kaiju", ["Blunt", "Piloted", "Stomp", "Thrust"], scale=None, mass=80000, accent=BLUE,
    feat=["Armor", "Turbine", "Cockpit"]), "Pacific Rim", S.gipsy_danger())
E(["knifehead", "pacific rim kaiju", "otachi", "slattern"],
  R("Pacific Rim Kaiju", "Kaiju", ["ArticulatedTail", "Charge", "Grab"], el="Poison", scale=16.0, mass=80000,
    accent=CYAN, temp="Aggressive", inten=0.9), "Pacific Rim")
E(["t-rex", "t rex", "trex", "tyrannosaurus", "tyrannosaurus rex", "rexy", "jurassic park t-rex"],
  R("T-Rex", "Kaiju", ["ArticulatedTail", "Charge", "Grab", "Stomp"], scale=12.0, mass=9000, accent=WOOD,
    temp="Aggressive", inten=0.7), "Jurassic Park / Jurassic World", S.t_rex())
E(["indominus rex", "indominus", "i-rex"],
  R("Indominus Rex", "Kaiju", ["ArticulatedTail", "Charge", "Grab"], scale=11.0, mass=10000, accent=WHITE,
    temp="Sneaky", inten=0.9), "Jurassic World")
E(["distortus rex", "d-rex", "mutant t-rex"],
  R("Distortus Rex", "Kaiju", ["ArticulatedTail", "Grab", "Stomp"], scale=12.0, mass=12000, accent=SILVER,
    temp="Chaotic", inten=0.9), "Jurassic World Rebirth")
E(["velociraptor", "raptor", "raptors", "blue the raptor", "velociraptors", "raptor squad"],
  R("Velociraptors", "Biped", ["Blade", "Charge", "Fast", "Swarm"], scale=1.6, mass=40, count=3, accent=BLUE,
    temp="Sneaky", inten=0.8), "Jurassic Park / Jurassic World")
E(["mosasaurus", "jurassic world mosasaurus"],
  R("Mosasaurus", "Fish", ["Charge", "Grab"], el="Water", scale=12.0, mass=20000, accent=BLUE,
    temp="Aggressive", inten=0.8), "Jurassic World")
E(["quetzalcoatlus", "pteranodon", "jurassic pterosaur"],
  R("Quetzalcoatlus", "Bird", ["Charge", "Flies", "Grab"], el="Wind", scale=4.0, mass=250, accent=WOOD,
    temp="Aggressive", inten=0.6), "Jurassic World")
E(["shai-hulud", "sandworm", "sand worm", "shai hulud", "dune worm", "arrakis sandworm"],
  R("Shai-Hulud", "Kaiju", ["Charge", "Grab", "Stomp"], el="Earth", scale=20.0, mass=100000, accent=ORANGE,
    temp="Aggressive", inten=0.6), "Dune", S.sandworm())
E(["ornithopter", "dune ornithopter", "thopter"],
  R("Ornithopter", "Vehicle", ["Flies", "Piloted", "Shoots"], el="Wind", scale=8.0, mass=6000, accent=ORANGE,
    feat=["Rotor", "Cockpit"]), "Dune")
E(["dune thumper"],
  R("Thumper", "Pillar", ["Attract", "Stomp"], el="Earth", scale=2.0, mass=20, accent=ORANGE, feat=["Pistons"]), "Dune")
E(["xenomorph", "alien xenomorph", "xenomorphe", "the alien", "xeno", "alien queen"],
  R("Xenomorph", "Biped", ["ArticulatedTail", "Fast", "Grab", "Pierce"], el="Poison", scale=None, mass=150,
    accent=BLACK, temp="Sneaky", inten=0.9), "Alien / Alien: Earth", S.xenomorph())
E(["facehugger", "face hugger", "facehuggers"],
  R("Facehuggers", "Arachnid", ["Fast", "Grab", "Sticky", "Swarm"], el="Poison", scale=0.6, mass=1, count=4,
    accent=WOOD, temp="Aggressive", inten=0.8), "Alien")
E(["predator plasma caster", "yautja", "plasma caster", "predator badlands"],
  R("Predator's Plasma Caster", "Gun", ["Explodes", "Homing", "Shoots"], el="Electric", scale=1.6, mass=6,
    accent=CYAN), "Predator / Predator: Badlands")
E(["predator smart disc", "smart disc", "predator disc"],
  R("Smart Disc", "Disc", ["Blade", "Homing", "Returning"], scale=1.2, mass=2, accent=SILVER), "Predator")
E(["bruce the shark", "great white shark jaws", "jaws shark", "jaws the shark"],
  R("Jaws", "Fish", ["Charge", "Grab"], el="Water", scale=7.0, mass=3000, accent=SILVER,
    temp="Aggressive", inten=0.9), "Jaws")
E(["sharknado", "shark tornado", "shark-nado"],
  R("Sharknado", "Fish", ["Grab", "Spin", "Swarm"], el="Wind", scale=1.8, mass=60, count=8, accent=BLUE,
    temp="Chaotic", inten=1.0), "Sharknado")

# =============================================================== STRANGER THINGS / WEDNESDAY / NETFLIX
E(["demogorgon", "the demogorgon", "demagorgon", "stranger things monster"],
  R("Demogorgon", "Biped", ["Fast", "Grab", "Pierce"], scale=None, mass=150, accent=RED,
    temp="Sneaky", inten=0.9), "Stranger Things", S.demogorgon())
E(["demodogs", "demodog", "demo dogs"],
  R("Demodogs", "Quadruped", ["Charge", "Fast", "Swarm"], scale=1.2, mass=40, count=4, accent=RED,
    temp="Aggressive", inten=0.8), "Stranger Things")
E(["mind flayer", "the mind flayer", "shadow monster"],
  R("Mind Flayer", "Kaiju", ["Constrict", "Grab", "Stomp"], el="Shadow", scale=20.0, mass=50000, accent=RED,
    temp="Chaotic", inten=0.9), "Stranger Things")
E(["vecna", "henry creel"],
  R("Vecna", "Biped", ["Grab", "Pull"], el="Psychic", scale=2.4, mass=80, accent=RED,
    temp="Sneaky", inten=0.9), "Stranger Things")
E(["eleven's powers", "el hopper", "telekinesis eleven"],
  R("Eleven's Telekinesis", "Gauntlet", ["Grab", "Repel"], el="Psychic", scale=1.4, mass=1, accent=PINK),
  "Stranger Things")
E(["thing addams", "wednesday's thing", "thing the hand", "addams family thing", "crawling hand"],
  R("Thing", "Arachnid", ["Fast", "Grab"], scale=0.8, mass=1, accent=WHITE, temp="Sneaky", inten=0.8),
  "Wednesday / The Addams Family")
E(["squid game doll", "younghee", "young-hee", "red light green light doll", "squid game"],
  R("Young-hee Doll", "Pillar", ["Beam", "Shoots"], scale=8.0, mass=3000, accent=ORANGE), "Squid Game")
E(["squid game guard", "pink guard", "pink soldiers"],
  R("Pink Guards", "Biped", ["Shoots", "Swarm"], scale=1.8, mass=70, count=4, accent=PINK), "Squid Game")
E(["honmoon", "the honmoon", "golden honmoon"],
  R("Honmoon", "Dome", ["Barrier", "Repel"], el="Light", scale=6.0, mass=100, accent=GOLD), "KPop Demon Hunters")
E(["rumi's sword", "saingeom", "rumi sword", "kpop demon hunters sword", "huntrix sword"],
  R("Rumi's Saingeom", "Sword", ["Blade", "Fast"], el="Psychic", scale=2.4, mass=2, accent=PURPLE),
  "KPop Demon Hunters")
E(["mira's gok-do", "gok-do", "gokdo", "mira glaive"],
  R("Mira's Gok-do", "Spear", ["Blade", "Pierce", "Spin"], scale=3.2, mass=4, accent=PINK), "KPop Demon Hunters")
E(["zoey's shinkal", "shinkal", "zoey knives", "throwing knives kpop"],
  R("Zoey's Shinkal", "Disc", ["Blade", "Swarm", "Homing"], scale=0.8, mass=0.5, count=4, accent=CYAN),
  "KPop Demon Hunters")
E(["derpy tiger", "derpy", "kpop demon hunters tiger"],
  R("Derpy Tiger", "Quadruped", ["Charge", "Bounce"], scale=2.0, mass=150, accent=BLUE, temp="Playful", inten=1.0),
  "KPop Demon Hunters")
E(["homelander", "homelander laser eyes", "the boys homelander"],
  R("Homelander", "Biped", ["Beam", "Fast", "Flies"], el="Fire", scale=2.0, mass=100, accent=BLUE,
    temp="Aggressive", inten=1.0), "The Boys")
E(["mark grayson", "omni-man", "omniman", "nolan grayson"],
  R("Invincible", "Biped", ["Blunt", "Charge", "Flies"], scale=2.0, mass=90, accent=YELLOW,
    temp="Aggressive", inten=0.7), "Invincible")
E(["fishbones", "jinx's rocket launcher", "jinx rocket launcher", "fish bones"],
  R("Fishbones", "Cannon", ["Explodes", "Homing"], scale=None, mass=20, accent=PINK,
    payload=R("Fishbones Rocket", "Rocket", ["Explodes", "Homing"], el="Fire", scale=1.0, mass=3)), "Arcane", S2.fishbones())
E(["flame chompers", "jinx chompers", "chomper grenades"],
  R("Flame Chompers", "Bomb", ["Explodes", "Swarm", "Walks"], el="Fire", scale=0.6, mass=1, count=3, accent=PINK,
    temp="Chaotic", inten=0.9), "Arcane")
E(["vi's gauntlets", "atlas gauntlets", "vi gauntlets", "hextech gauntlets", "vi arcane"],
  R("Atlas Gauntlets", "Gauntlet", ["Blunt", "Heavy", "Thrust"], el="Electric", scale=2.2, mass=40, accent=PINK),
  "Arcane")
E(["mercury hammer", "jayce's hammer", "hextech hammer", "jayce hammer"],
  R("Mercury Hammer", "Hammer", ["Blunt", "Shoots"], el="Electric", scale=2.8, mass=40, accent=CYAN), "Arcane")

# =============================================================== GHOSTBUSTERS / BTTF / DR WHO / STAR TREK / TIME
E(["proton pack", "neutrona wand", "ghostbusters proton pack", "proton gun", "ghostbusters gun", "proton stream"],
  R("Proton Pack", "Gun", ["Beam", "Pull"], el="Electric", scale=None, mass=15, accent=ORANGE),
  "Ghostbusters", S.neutrona_wand())
E(["ghost trap", "ghostbusters trap"],
  R("Ghost Trap", "Cage", ["Attract", "Trap"], el="Electric", scale=2.0, mass=10, accent=SILVER), "Ghostbusters")
E(["stay puft marshmallow man", "stay puft", "stay-puft", "marshmallow man", "staypuft", "stay puft man"],
  R("Stay Puft Marshmallow Man", "Kaiju", ["Bounce", "Stomp", "Sticky"], scale=None, mass=30000, accent=WHITE,
    temp="Playful", inten=0.8), "Ghostbusters", S.stay_puft())
E(["slimer", "onionhead ghost"],
  R("Slimer", "Blob", ["Flies", "Sticky"], el="Poison", scale=1.4, mass=5, accent=GREEN, temp="Playful", inten=0.9),
  "Ghostbusters")
E(["ecto-1", "ecto 1", "ecto1", "ghostbusters car"],
  R("Ecto-1", "Vehicle", ["Charge", "Piloted"], scale=6.0, mass=2500, accent=WHITE,
    feat=["Wheels", "Headlight", "Antenna", "Windows"]), "Ghostbusters")
E(["delorean", "de lorean", "time machine car", "back to the future car", "dmc delorean", "delorian"],
  R("DeLorean Time Machine", "Vehicle", ["Charge", "Fast", "Piloted", "Portal"], el="Electric", scale=None,
    mass=1300, accent=SILVER, feat=["Wheels", "Exhaust"]), "Back to the Future", S.delorean())
E(["hoverboard", "hover board", "marty's hoverboard", "mattel hoverboard"],
  R("Hoverboard", "Vehicle", ["Bounce", "Fast", "Piloted"], scale=1.2, mass=3, accent=PINK), "Back to the Future Part II")
E(["tardis", "the tardis", "police box", "t.a.r.d.i.s."],
  R("TARDIS", "Pillar", ["Barrier", "Portal"], scale=None, mass=5000, accent=BLUE), "Doctor Who", S.tardis())
E(["sonic screwdriver", "sonic screw driver", "the doctor's screwdriver"],
  R("Sonic Screwdriver", "Staff", ["Beam", "Repel"], el="Electric", scale=None, mass=0.3, accent=CYAN),
  "Doctor Who", S.sonic_screwdriver())
E(["dalek", "daleks", "exterminator robot"],
  R("Dalek", "Vehicle", ["Beam", "Homing"], scale=None, mass=400, accent=GOLD, feat=["Armor"],
    temp="Aggressive", inten=0.9), "Doctor Who", S.dalek())
E(["cybermen", "cyberman"],
  R("Cybermen", "Biped", ["Beam", "Swarm"], el="Electric", scale=2.0, mass=150, count=3, accent=SILVER,
    feat=["Armor"], temp="Aggressive", inten=0.6), "Doctor Who")
E(["weeping angel", "weeping angels"],
  R("Weeping Angels", "Biped", ["Fast", "Grab", "Swarm"], el="Earth", scale=2.0, mass=500, count=3, accent=SILVER,
    temp="Sneaky", inten=1.0), "Doctor Who")
E(["uss enterprise", "starship enterprise", "ncc-1701"],
  R("USS Enterprise", "Vehicle", ["Beam", "Flies", "Piloted", "Shoots"], scale=None, mass=100000, accent=SILVER,
    feat=["Dish", "Windows", "Jet"]), "Star Trek", S2.enterprise())
E(["phaser", "star trek phaser", "phasers"],
  R("Phaser", "Gun", ["Beam"], el="Electric", scale=1.0, mass=1, accent=SILVER), "Star Trek")
E(["tractor beam", "star trek tractor beam"],
  R("Tractor Beam", "Staff", ["Beam", "Pull"], scale=2.4, mass=5, accent=CYAN), "Star Trek / Star Wars")
E(["tribble", "tribbles", "the trouble with tribbles"],
  R("Tribbles", "Ball", ["Bounce", "Swarm"], scale=0.5, mass=0.5, count=12, accent=WOOD, temp="Playful", inten=0.7),
  "Star Trek")
E(["borg cube", "the borg"],
  R("Borg Cube", "Cage", ["Attract", "DropFromSky", "Trap"], el="Electric", scale=12.0, mass=90000, accent=GREEN),
  "Star Trek")
E(["bat'leth", "batleth", "klingon sword"],
  R("Bat'leth", "Sword", ["Blade", "Guard"], scale=2.4, mass=5, accent=SILVER), "Star Trek")

# =============================================================== CARTOONS / ANIMATION
E(["acme anvil", "anvil acme", "looney tunes anvil", "cartoon anvil", "wile e coyote anvil", "acme"],
  R("ACME Anvil", "Anvil", ["Blunt", "DropFromSky", "Heavy"], scale=None, mass=500, accent=BLACK),
  "Looney Tunes", S.acme_anvil())
E(["acme rocket", "wile e coyote rocket", "acme rocket skates"],
  R("ACME Rocket", "Rocket", ["Explodes", "Thrust"], el="Fire", scale=2.4, mass=20, accent=RED,
    temp="Chaotic", inten=0.8), "Looney Tunes")
E(["portable hole", "acme portable hole"],
  R("Portable Hole", "Platform", ["Portal"], el="Shadow", scale=2.4, mass=1, accent=BLACK), "Looney Tunes")
E(["acme dynamite", "looney tunes dynamite", "tnt acme"],
  R("ACME Dynamite", "Bomb", ["Explodes", "Sticky"], el="Fire", scale=1.2, mass=2, accent=RED), "Looney Tunes")
E(["taz", "tasmanian devil", "taz tornado"],
  R("Tasmanian Devil", "Biped", ["Charge", "Spin"], el="Wind", scale=1.6, mass=40, accent=WOOD,
    temp="Chaotic", inten=1.0), "Looney Tunes")
E(["road runner", "roadrunner"],
  R("Road Runner", "Bird", ["Charge", "Fast"], scale=1.2, mass=2, accent=BLUE, temp="Playful", inten=0.9),
  "Looney Tunes")
E(["illudium q-36 explosive space modulator", "marvin the martian", "marvin's modulator", "space modulator"],
  R("Illudium Q-36", "Bomb", ["Explodes", "DropFromSky"], el="Atomic", scale=1.4, mass=5, accent=GREEN),
  "Looney Tunes")
E(["spongebob", "spongebob squarepants", "sponge bob"],
  R("SpongeBob", "Blob", ["Bounce", "Sticky"], el="Water", scale=None, mass=5, accent=YELLOW, temp="Playful", inten=1.0),
  "SpongeBob SquarePants", S2.spongebob())
E(["krabby patty", "krabby patties", "crabby patty"],
  R("Krabby Patty", "Ball", ["Bounce"], scale=None, mass=1, count=3, accent=WOOD), "SpongeBob SquarePants", S2.krabby_patty())
E(["jellyfish net", "spongebob net"],
  R("Jellyfish Net", "Staff", ["Trap", "Pull"], scale=2.2, mass=2, accent=PINK), "SpongeBob SquarePants")
E(["portal gun", "rick's portal gun", "rick and morty portal gun", "portalgun", "rick sanchez gun"],
  R("Portal Gun", "Gun", ["Portal"], scale=None, mass=2, accent=GREEN), "Rick and Morty", S.portal_gun())
E(["mr meeseeks", "meeseeks", "meeseeks box", "mr. meeseeks"],
  R("Mr. Meeseeks", "Biped", ["Grab", "Swarm"], scale=1.8, mass=60, count=5, accent=CYAN, temp="Chaotic", inten=0.9),
  "Rick and Morty")
E(["pickle rick", "pickle ricks"],
  R("Pickle Rick", "Blob", ["Fast", "Ricochet", "Bounce"], scale=0.8, mass=1, accent=GREEN, temp="Aggressive", inten=0.8),
  "Rick and Morty")
E(["jake the dog", "jake adventure time"],
  R("Jake the Dog", "Quadruped", ["Constrict", "Grab", "Grow"], scale=1.8, mass=30, accent=YELLOW,
    temp="Playful", inten=0.8), "Adventure Time")
E(["finn's sword", "demon blood sword", "finn the human sword", "grass sword"],
  R("Finn's Sword", "Sword", ["Blade"], scale=2.2, mass=3, accent=RED), "Adventure Time")
E(["bluey", "bluey heeler", "bingo heeler"],
  R("Bluey", "Quadruped", ["Bounce", "Fast"], scale=1.0, mass=15, accent=BLUE, temp="Playful", inten=1.0), "Bluey")
E(["keepy uppy", "keepy uppy balloon", "bluey balloon"],
  R("Keepy Uppy Balloon", "Ball", ["Bounce", "SlowFall"], scale=1.2, mass=0.2, accent=RED), "Bluey")
E(["homer's donut", "pink donut simpsons", "lard lad donut"],
  R("Lard Lad Donut", "Disc", ["Bounce", "Ricochet"], scale=2.0, mass=2, accent=PINK), "The Simpsons")
E(["spider-pig", "spider pig", "plopper"],
  R("Spider-Pig", "Quadruped", ["Sticky", "Pull"], scale=1.2, mass=40, accent=PINK, temp="Playful", inten=0.8),
  "The Simpsons")

# ---- Disney / Pixar / DreamWorks / Illumination
E(["minion", "minions", "the minions", "bob the minion", "kevin the minion", "stuart the minion", "minyon"],
  R("Minions", "Biped", ["Swarm"], scale=None, mass=15, count=6, accent=BLUE, temp="Chaotic", inten=0.9),
  "Despicable Me / Minions & Monsters", S.minion())
E(["freeze ray", "gru's freeze ray", "gru freeze ray"],
  R("Gru's Freeze Ray", "Gun", ["Beam"], el="Ice", scale=1.8, mass=6, accent=CYAN), "Despicable Me")
E(["shrink ray", "gru's shrink ray", "moon shrink ray"],
  R("Shrink Ray", "Gun", ["Beam", "Shrink"], scale=1.8, mass=6, accent=SILVER), "Despicable Me")
E(["growth ray", "grow ray", "embiggening ray"],
  R("Growth Ray", "Gun", ["Beam", "Grow"], scale=1.8, mass=6, accent=GREEN), "Despicable Me / cartoons")
E(["fart gun", "gru fart gun"],
  R("Fart Gun", "Gun", ["Repel", "Shoots"], el="Poison", scale=1.6, mass=3, accent=GREEN), "Despicable Me")
E(["stitch", "experiment 626", "stich", "lilo and stitch"],
  R("Stitch", "Biped", ["Charge", "Fast", "Shoots"], scale=None, mass=40, accent=CYAN, temp="Chaotic", inten=0.9,
    payload=R("Plasma Bolt", "Ball", ["Explodes"], el="Electric", scale=0.4, mass=0.5)),
  "Lilo & Stitch (2025)", S.stitch())
E(["toothless dragon", "night fury", "toothless the dragon", "how to train your dragon"],
  R("Toothless", "Quadruped", ["Flies", "Piloted", "Shoots"], scale=4.0, mass=800, accent=RED,
    temp="Protective", inten=0.7,
    payload=R("Plasma Blast", "Ball", ["Explodes"], el="Fire", scale=0.8, mass=1)),
  "How to Train Your Dragon (2025)", S.toothless())
E(["buzz lightyear wings", "buzz lightyear", "buzz wings"],
  R("Buzz Lightyear's Wings", "Wings", ["Beam", "Glide"], scale=3.6, mass=4, accent=PURPLE), "Toy Story 5")
E(["the claw", "toy story claw"],
  R("The Claw", "Gauntlet", ["DropFromSky", "Grab"], scale=None, mass=30, accent=SILVER), "Toy Story", S2.the_claw())
E(["up house", "balloon house", "carl's house", "up balloons"],
  R("Up House", "Platform", ["Flies", "SlowFall"], el="Wind", scale=None, mass=3000, accent=PINK,
    feat=["Windows"]), "Up (Pixar)", S2.up_house())
E(["lightning mcqueen", "mcqueen"],
  R("Lightning McQueen", "Vehicle", ["Charge", "Fast"], scale=4.5, mass=1300, accent=RED,
    feat=["Wheels", "Headlight"], temp="Playful", inten=0.6), "Cars (Pixar)")
E(["wall-e", "walle", "wall e"],
  R("WALL-E", "Vehicle", ["Grab", "Charge"], scale=None, mass=60, accent=GOLD, feat=["Treads"],
    temp="Protective", inten=0.7), "WALL-E (Pixar)", S2.wall_e())
E(["baymax", "baymax armor", "big hero 6"],
  R("Baymax", "Biped", ["Bounce", "Flies", "Thrust"], scale=None, mass=40, accent=WHITE, temp="Protective", inten=1.0),
  "Big Hero 6", S2.baymax())
E(["maui's fish hook", "maui hook", "magic fish hook", "maui", "mauis hook"],
  R("Maui's Fish Hook", "Staff", ["Grab", "Pull"], scale=None, mass=15, accent=WOOD), "Moana (2026)", S.maui_hook())
E(["te ka", "lava monster moana"],
  R("Te Kā", "Kaiju", ["Breath", "Stomp"], el="Fire", scale=18.0, mass=80000, accent=ORANGE,
    temp="Aggressive", inten=0.9), "Moana")
E(["monsters inc door", "scare door", "closet door portal", "boo's door"],
  R("Scare Door", "Platform", ["Portal"], scale=2.6, mass=20, accent=PINK), "Monsters, Inc.")
E(["elsa's ice palace", "elsa"],
  R("Elsa's Ice Palace", "Dome", ["Barrier", "Launch"], el="Ice", scale=6.0, mass=2000, accent=CYAN), "Frozen")
E(["olaf", "olaf snowman"],
  R("Olaf", "Biped", ["Bounce"], el="Ice", scale=1.2, mass=10, accent=WHITE, temp="Playful", inten=1.0), "Frozen")
E(["shrek", "ogre shrek"],
  R("Shrek", "Biped", ["Blunt", "Heavy", "Charge"], el="Earth", scale=None, mass=160, accent=GREEN,
    temp="Lazy", inten=0.5), "Shrek", S2.shrek())
E(["donkey shrek", "shrek's donkey"],
  R("Donkey", "Quadruped", ["Charge", "Fast"], scale=1.4, mass=150, accent=SILVER, temp="Playful", inten=1.0), "Shrek")
E(["gingy", "gingerbread man shrek", "gingerbread man"],
  R("Gingy", "Biped", ["Fast"], scale=0.8, mass=1, count=3, accent=WOOD, temp="Fearful", inten=0.7), "Shrek")
E(["dragon from shrek", "shrek dragon", "pink dragon shrek"],
  R("Dragon (Shrek)", "Kaiju", ["Breath", "Flies", "Grab"], el="Fire", scale=12.0, mass=20000, accent=PINK,
    temp="Protective", inten=0.6), "Shrek")
E(["puss in boots", "puss in boots sword"],
  R("Puss in Boots", "Quadruped", ["Blade", "Fast"], scale=1.0, mass=5, accent=ORANGE, temp="Sneaky", inten=0.6),
  "Shrek / Puss in Boots")
E(["universe ring", "qiankun circle", "qiankun ring", "nezha ring", "ne zha ring"],
  R("Universe Ring", "Disc", ["Blunt", "Heavy", "Ricochet"], el="Fire", scale=None, mass=30, accent=GOLD), "Ne Zha 2", S2.universe_ring())
E(["fire-tipped spear", "nezha spear", "ne zha spear", "fire tipped spear"],
  R("Fire-Tipped Spear", "Spear", ["Pierce", "Spin"], el="Fire", scale=3.2, mass=6, accent=RED), "Ne Zha 2")
E(["wind fire wheels", "nezha wheels", "ne zha wind fire wheels"],
  R("Wind Fire Wheels", "Vehicle", ["Fast", "Flies", "Piloted"], el="Fire", scale=1.2, mass=5, accent=ORANGE,
    feat=["Wheels"]), "Ne Zha 2")
E(["ikran", "banshee avatar", "mountain banshee", "avatar banshee"],
  R("Ikran", "Bird", ["Charge", "Flies", "Piloted"], scale=4.0, mass=300, accent=CYAN), "Avatar: Fire and Ash")
E(["toruk", "toruk makto", "great leonopteryx"],
  R("Toruk", "Kaiju", ["Flies", "Grab", "Piloted"], el="Fire", scale=12.0, mass=5000, accent=RED,
    temp="Aggressive", inten=0.7), "Avatar")
E(["amp suit", "avatar mech", "amp mech"],
  R("AMP Suit", "Biped", ["Blade", "Piloted", "Shoots"], scale=4.0, mass=3000, accent=SILVER,
    feat=["Armor", "Cockpit", "Barrels"]), "Avatar")
E(["tulkun", "payakan", "avatar whale"],
  R("Tulkun", "Fish", ["Charge", "Repel"], el="Water", scale=14.0, mass=60000, accent=BLUE,
    temp="Protective", inten=0.8), "Avatar: The Way of Water")
E(["trojan horse", "wooden horse of troy", "the odyssey horse"],
  R("Trojan Horse", "Quadruped", ["Heavy", "Shoots", "Walks"], scale=None, mass=8000, accent=WOOD, feat=["Wheels"],
    payload=R("Greek Soldiers", "Biped", ["Swarm"], scale=1.8, mass=80, count=4)), "The Odyssey (2026)", S2.trojan_horse())
E(["cyclops polyphemus", "polyphemus", "the odyssey cyclops"],
  R("Polyphemus", "Kaiju", ["Grab", "Stomp"], el="Earth", scale=14.0, mass=40000, accent=WOOD,
    temp="Aggressive", inten=0.8), "The Odyssey (2026)")
E(["scylla"],
  R("Scylla", "Serpent", ["Grab", "Swarm"], el="Water", scale=8.0, mass=3000, count=6, accent=BLUE,
    temp="Aggressive", inten=0.8), "The Odyssey (2026)")
E(["bow of odysseus", "odysseus bow", "odysseus"],
  R("Bow of Odysseus", "Bow", ["Pierce", "Ricochet"], scale=2.4, mass=4, accent=WOOD), "The Odyssey (2026)")
E(["rocky the eridian", "rocky eridian", "eridian", "rocky project hail mary"],
  R("Rocky", "Arachnid", ["Grab", "Guard"], scale=1.8, mass=170, accent=WOOD, temp="Protective", inten=0.9),
  "Project Hail Mary (2026)")
E(["astrophage", "astrophage swarm"],
  R("Astrophage", "Blob", ["Attract", "Swarm"], el="Fire", scale=0.4, mass=0.1, count=12, accent=ORANGE),
  "Project Hail Mary (2026)")
E(["power sword", "he-man sword", "sword of grayskull", "he man sword", "he-man"],
  R("Power Sword", "Sword", ["Blade", "Heavy"], el="Electric", scale=None, mass=6, accent=GOLD),
  "Masters of the Universe (2026)", S.power_sword())
E(["battle cat", "cringer"],
  R("Battle Cat", "Quadruped", ["Charge", "Piloted"], scale=2.4, mass=400, accent=GREEN, feat=["Armor"]),
  "Masters of the Universe")
E(["havoc staff", "skeletor's staff", "skeletor"],
  R("Havoc Staff", "Staff", ["Beam", "Pull"], el="Shadow", scale=2.8, mass=5, accent=PURPLE), "Masters of the Universe")
E(["castle grayskull", "grayskull"],
  R("Castle Grayskull", "Wall", ["Barrier", "Walks"], el="Earth", scale=10.0, mass=90000, accent=GREEN),
  "Masters of the Universe")

# ---- Transformers / mechs / misc live action
E(["optimus prime", "optimus", "autobot truck", "optimus prime truck"],
  R("Optimus Prime", "Vehicle", ["Charge", "Shoots", "Walks"], scale=None, mass=9000, accent=RED,
    feat=["Wheels", "Cab", "Smokestack", "Armor"], temp="Protective", inten=0.8), "Transformers", S2.optimus_prime())
E(["bumblebee transformer", "bumblebee camaro"],
  R("Bumblebee", "Vehicle", ["Fast", "Shoots", "Walks"], scale=4.5, mass=1600, accent=YELLOW,
    feat=["Wheels", "Armor"], temp="Playful", inten=0.6), "Transformers")
E(["megatron", "fusion cannon", "megatron's fusion cannon"],
  R("Fusion Cannon", "Cannon", ["Explodes", "Heavy"], el="Atomic", scale=2.6, mass=60, accent=SILVER),
  "Transformers")
E(["devastator transformer", "constructicons", "devastator transformers"],
  R("Devastator", "Kaiju", ["Grab", "Heavy", "Stomp"], scale=18.0, mass=90000, accent=GREEN,
    feat=["Treads", "CraneArm", "Drill"], temp="Aggressive", inten=0.8), "Transformers")
E(["megazord", "power rangers megazord", "thunderzord"],
  R("Megazord", "Kaiju", ["Blade", "Piloted", "Stomp"], scale=16.0, mass=80000, accent=RED,
    feat=["Armor", "Cockpit"]), "Power Rangers")
E(["voltron", "blazing sword voltron"],
  R("Voltron", "Kaiju", ["Blade", "Flies", "Piloted"], scale=18.0, mass=90000, accent=BLUE,
    feat=["Armor", "Cockpit", "Jet"]), "Voltron")
E(["iron giant", "the iron giant"],
  R("The Iron Giant", "Kaiju", ["Beam", "Flies", "Grab"], scale=12.0, mass=40000, accent=SILVER,
    temp="Protective", inten=0.9), "The Iron Giant")
E(["terminator", "t-800", "the terminator"],
  R("Terminator", "Biped", ["Heavy", "Shoots"], scale=2.0, mass=300, accent=SILVER, feat=["Armor"],
    temp="Aggressive", inten=1.0), "The Terminator")
E(["t-1000", "liquid metal terminator", "t1000"],
  R("T-1000", "Blob", ["Blade", "Fast", "Sticky"], scale=2.0, mass=150, accent=SILVER, temp="Sneaky", inten=0.9),
  "Terminator 2")
E(["light cycle", "tron light cycle", "lightcycle", "tron bike"],
  R("Light Cycle", "Vehicle", ["Barrier", "Fast", "Piloted"], el="Electric", scale=None, mass=200, accent=CYAN,
    feat=["Wheels"]), "Tron: Ares", S2.light_cycle())
E(["identity disc", "tron disc", "tron frisbee"],
  R("Identity Disc", "Disc", ["Blade", "Returning", "Ricochet"], el="Electric", scale=1.4, mass=1, accent=CYAN),
  "Tron")
E(["indiana jones whip", "indy's whip", "indiana jones", "bullwhip indy"],
  R("Indiana Jones' Whip", "Whip", ["Link", "Pull"], scale=4.0, mass=1.5, accent=WOOD), "Indiana Jones")
E(["indiana jones boulder", "raiders boulder", "temple boulder"],
  R("Rolling Temple Boulder", "Boulder", ["Blunt", "Charge", "Heavy"], el="Earth", scale=5.0, mass=9000, accent=WOOD),
  "Raiders of the Lost Ark")
E(["holy hand grenade", "holy hand grenade of antioch"],
  R("Holy Hand Grenade", "Bomb", ["Explodes"], el="Light", scale=1.2, mass=2, accent=GOLD),
  "Monty Python and the Holy Grail")
E(["mary poppins umbrella", "mary poppins", "parrot umbrella"],
  R("Mary Poppins' Umbrella", "Staff", ["Glide", "SlowFall"], el="Wind", scale=2.4, mass=1, accent=BLACK),
  "Mary Poppins")
E(["gremlins", "gremlin", "gizmo mogwai"],
  R("Gremlins", "Biped", ["Fast", "Swarm"], scale=0.8, mass=5, count=6, accent=GREEN, temp="Chaotic", inten=1.0),
  "Gremlins")
E(["war rig", "mad max war rig", "fury road truck"],
  R("War Rig", "Vehicle", ["Charge", "Heavy", "Piloted"], el="Fire", scale=10.0, mass=20000, accent=ORANGE,
    feat=["Wheels", "Spikes", "Exhaust", "Cab"]), "Mad Max: Fury Road")

from ftlib import ROOT
out_path = sys.argv[1] if len(sys.argv) > 1 else os.path.join(ROOT, "assets", "icons", "film_tv.ron")
# Transformers are owned by assets/icons/transformers.ron (tools/icons/transformers) — never emit them here.
ENTRIES[:] = [e for e in ENTRIES if "Transformers" not in repr(e)]
names_count, shaped = emit(out_path, drop_same={"boxing glove construct", "toothless dragon"})
print(names_count, "entries,", shaped, "shaped")
