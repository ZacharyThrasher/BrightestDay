"""Words for derived kits: element / temperament / flavour vocabularies (derive_kits.py).

Every name here is a MOVE name: "<character> <word of it>" fires that move, so these words must not
be ones the costume grammar reads after a character ("goku sword", "gojo sunglasses", "red goku",
"giant goku") — derive_kits.py rejects any that collide and falls back to the next choice.
"""

# Element → presentation style (assets/tuning/move_fx.ron), status, colour and vocabulary.
ELEM = {
    "Fire": dict(fx="fire", status="Burn", color=(1.0, 0.45, 0.1),
                 adj=["blazing", "searing", "scorching", "infernal", "ember"],
                 bolt=["fireball", "flame bolt"], ray=["heat ray", "flame lance"], breath=["fire breath", "flame breath"],
                 storm=["meteor storm", "inferno", "firestorm"], ward=["flame ward", "heat shimmer"], step=["flame step", "blaze step"],
                 noun="flame"),
    "Electric": dict(fx="lightning", status="Stun", color=(0.62, 0.8, 1.0),
                     adj=["thunder", "lightning", "volt", "shock", "static"],
                     bolt=["thunderbolt", "shock bolt"], ray=["lightning arc", "volt ray"], breath=["lightning breath", "shock breath"],
                     storm=["thunderstorm", "lightning storm", "wrath of thunder"], ward=["static field", "volt ward"],
                     step=["lightning step", "volt step"], noun="thunder"),
    "Ice": dict(fx="ice", status="Chill", color=(0.6, 0.85, 1.0),
                adj=["frost", "glacial", "frozen", "rime", "hailing"],
                bolt=["ice shard", "frost spike"], ray=["freeze ray", "frost ray"], breath=["frost breath", "ice breath"],
                storm=["blizzard", "absolute zero", "deep freeze"], ward=["frost ward", "ice ward"], step=["ice slide", "frost step"],
                noun="frost"),
    "Water": dict(fx="water", status="Wet", color=(0.3, 0.6, 1.0),
                  adj=["tidal", "torrent", "riptide", "hydro", "surging"],
                  bolt=["water blast", "hydro bolt"], ray=["hydro cannon", "torrent ray"], breath=["water spout", "torrent breath"],
                  storm=["tidal wave", "maelstrom", "tsunami"], ward=["water veil", "tide ward"], step=["riptide dash", "undertow"],
                  noun="tide"),
    "Wind": dict(fx="wind", status=None, color=(0.75, 0.95, 0.85),
                 adj=["gale", "cyclone", "tempest", "squall", "whirlwind"],
                 bolt=["wind cutter", "gale bolt"], ray=["air cannon", "vacuum wave"], breath=["gale breath", "howling breath"],
                 storm=["tornado", "hurricane", "cyclone"], ward=["wind veil", "gale ward"], step=["updraft", "gale step"],
                 noun="gale"),
    "Earth": dict(fx="earth", status=None, color=(0.6, 0.45, 0.3),
                  adj=["stone", "quake", "granite", "boulder", "tectonic"],
                  bolt=["rock throw", "stone bolt"], ray=["sandblast", "grit ray"], breath=["sand breath", "dust breath"],
                  storm=["earthquake", "landslide", "tectonic fury"], ward=["stone skin", "granite ward"], step=["burrow", "tremor step"],
                  noun="stone"),
    "Poison": dict(fx="poison", status="Poison", color=(0.4, 0.85, 0.2),
                   adj=["venom", "toxic", "acid", "plague", "noxious"],
                   bolt=["venom spit", "acid bolt"], ray=["acid ray", "toxic spray"], breath=["poison breath", "toxic breath"],
                   storm=["plague cloud", "toxic deluge", "pestilence"], ward=["toxic miasma", "venom ward"], step=["miasma step", "ooze slide"],
                   noun="venom"),
    "Psychic": dict(fx="psychic", status="Float", color=(0.95, 0.4, 0.9),
                    adj=["psychic", "mind", "psionic", "astral", "telekinetic"],
                    bolt=["psi bolt", "mind spike"], ray=["mind beam", "psybeam"], breath=["psychic roar", "mind howl"],
                    storm=["psychic storm", "mind crush", "astral collapse"], ward=["psychic barrier", "mind ward"],
                    step=["teleport", "astral step"], noun="mind"),
    "Shadow": dict(fx="shadow", status="Shadow", color=(0.35, 0.15, 0.5),
                   adj=["shadow", "umbral", "night", "dusk", "phantom"],
                   bolt=["shadow bolt", "night bolt"], ray=["umbral ray", "shadow lance"], breath=["shadow breath", "night breath"],
                   storm=["eclipse", "nightfall", "abyss"], ward=["shadow veil", "night ward"], step=["shadow step", "umbral step"],
                   noun="shadow"),
    "Atomic": dict(fx="atomic", status="Burn", color=(0.3, 1.0, 0.4),
                   adj=["atomic", "gamma", "nuclear", "fission", "reactor"],
                   bolt=["atomic blast", "gamma bolt"], ray=["gamma ray", "atomic ray"], breath=["atomic breath", "radiant breath"],
                   storm=["meltdown", "critical mass", "fallout"], ward=["radiation field", "gamma ward"], step=["quantum jump", "fission step"],
                   noun="atom"),
}

# Light-element characters: the flavour decides the words (and the style).
FLAVOUR = {
    "tech": dict(fx="hardlight", adj=["plasma", "ion", "pulse", "photon", "servo"], bolt=["plasma shot", "pulse blast"],
                 ray=["plasma beam", "ion ray"], storm=["orbital strike", "overclock", "full payload"], ward=["force field", "deflector"],
                 step=["thruster boost", "rocket boost"], noun="plasma"),
    "holy": dict(fx="holy", adj=["radiant", "holy", "divine", "solar", "blessed"], bolt=["radiant bolt", "holy bolt"],
                 ray=["solar beam", "radiant ray"], storm=["judgement", "sunburst", "wrath of heaven"], ward=["sanctuary", "holy ward"],
                 step=["radiant step", "ascension"], noun="radiance"),
    "mystic": dict(fx="psychic", adj=["arcane", "mystic", "eldritch", "spell", "hexing"], bolt=["arcane bolt", "mystic missile"],
                   ray=["arcane beam", "eldritch ray"], storm=["arcane storm", "spell barrage", "grand conjuration"],
                   ward=["arcane ward", "mystic barrier"], step=["blink", "arcane step"], noun="arcane"),
    "spirit": dict(fx="spirit", adj=["spectral", "ghostly", "haunting", "wailing", "ethereal"], bolt=["spirit bolt", "ghost fire"],
                   ray=["spectral ray", "soul lance"], storm=["haunting", "spirit storm", "wail of the dead"], ward=["spectral ward", "ghost veil"],
                   step=["phase step", "spectral step"], noun="spirit"),
    "chi": dict(fx="chi", adj=["chi", "spirit", "focused", "inner", "tiger"], bolt=["chi blast", "spirit palm"],
                ray=["chi beam", "spirit wave"], storm=["hundred palms", "heavenly strike", "spirit burst"], ward=["iron body", "chi ward"],
                step=["cloud step", "wind walk"], noun="chi"),
    "glitch": dict(fx="glitch", adj=["glitched", "pixel", "corrupted", "lagging", "cursed"], bolt=["pixel shot", "glitch bolt"],
                   ray=["glitch ray", "data beam"], storm=["system crash", "blue screen", "fatal error"], ward=["firewall", "error shield"],
                   step=["lag spike", "glitch step"], noun="glitch"),
    "speed": dict(fx="speedforce", adj=["blur", "lightspeed", "sonic", "turbo", "hyper"], bolt=["sonic boom", "speed blast"],
                  ray=["slipstream", "sonic wave"], storm=["overdrive", "lightspeed blitz", "mach storm"], ward=["speed vortex", "whirl guard"],
                  step=["blur dash", "sonic dash"], noun="speed"),
    "ink": dict(fx="ink", adj=["inky", "cartoon", "slapstick", "rubbery", "toon"], bolt=["ink blot", "cartoon bomb"],
                ray=["ink jet", "toon ray"], storm=["looney finale", "anvil drop", "toon mayhem"], ward=["toon guard", "rubber guard"],
                step=["scram", "toon dash"], noun="ink"),
}

# Temperament → how a PHYSICAL move is described (the personality of an unarmed / weapon move).
TEMPER_ADJ = {
    "Aggressive": ["savage", "brutal", "furious", "vicious", "ruthless"],
    "Calm": ["precise", "measured", "disciplined", "focused", "clean"],
    "Protective": ["valiant", "steadfast", "stalwart", "gallant", "loyal"],
    "Sneaky": ["silent", "cunning", "sly", "treacherous", "hidden"],
    "Playful": ["cheeky", "bouncy", "zany", "goofy", "prankster"],
    "Chaotic": ["wild", "unhinged", "manic", "mad", "reckless"],
    "Fearful": ["desperate", "panicked", "frantic", "nervous", "flailing"],
    "Lazy": ["lumbering", "sleepy", "slow burn", "drowsy", "heavy"],
}

# Keywords in a character's names / notes / franchise → hints (checked as whole words or word starts).
ELEMENT_WORDS = {
    "Fire": ["fire", "flame", "blaze", "burn", "heat", "inferno", "pyro", "magma", "lava", "ember", "scorch", "hellfire", "phoenix", "firestorm", "sun", "solar", "volcano", "torch"],
    "Ice": ["ice", "frost", "snow", "cold", "freeze", "frozen", "winter", "glacier", "blizzard", "iceman", "chill", "yeti", "frosty"],
    "Electric": ["thunder", "lightning", "storm", "volt", "electro", "electric", "shock", "static", "spark", "tesla", "zap", "bolt", "dynamo"],
    "Water": ["water", "aqua", "sea", "ocean", "tide", "wave", "rain", "hydro", "marine", "shark", "kraken", "squid", "octopus", "fish", "whale", "river", "tsunami"],
    "Wind": ["wind", "air", "gale", "tornado", "cyclone", "hurricane", "tempest", "whirlwind", "breeze", "sky", "storm"],
    "Earth": ["stone", "rock", "earth", "sand", "quake", "mud", "clay", "golem", "boulder", "mountain", "terra", "crystal", "diamond"],
    "Poison": ["poison", "venom", "toxic", "acid", "plague", "ooze", "slime", "sludge", "virus", "cobra", "viper", "swamp", "toxin"],
    "Psychic": ["mind", "psychic", "telepath*", "dream", "illusion", "hypno", "brain", "mental", "telekinetic", "oracle", "seer", "cosmic"],
    "Shadow": ["shadow", "dark", "night", "shade", "death", "dead", "grim", "crypt", "vampire", "nightmare", "undead", "zombie", "skull", "doom", "reaper", "demon", "devil", "hell", "raven", "necro", "lich", "wraith", "void", "abyss"],
    "Atomic": ["atomic", "nuclear", "gamma", "radiation", "atom", "reactor", "uranium", "plutonium", "hulk"],
}

FLAVOUR_WORDS = {
    "tech": ["robot", "cyborg", "android", "mech", "droid", "bot", "tech", "cyber", "iron", "machine", "suit", "armor", "armour",
             "drone", "gundam", "mobile", "unit", "model", "laser", "rocket", "titanfall", "mecha", "jaeger", "autobot", "decepticon",
             "transformer", "maximal", "predacon", "scientist", "inventor", "engineer", "hacker"],
    "holy": ["angel", "holy", "saint", "god", "goddess", "divine", "paladin", "cleric", "priest", "seraph", "celestial", "heaven",
             "heavenly", "light", "sun", "solar", "star", "valkyrie", "unicorn"],
    "mystic": ["wizard", "witch", "mage", "sorcerer", "sorceress", "sorcerer supreme", "magic", "magician", "warlock", "enchant",
               "spell", "shaman", "druid", "necromancer", "conjurer", "fairy", "fae", "elf", "genie", "djinn", "merlin", "hex", "rune",
               "alchemist", "oracle", "mystic"],
    "spirit": ["ghost", "spirit", "phantom", "specter", "spectre", "spook", "poltergeist", "banshee", "wraith", "soul", "haunt", "ghoul"],
    "chi": ["monk", "martial", "kung", "karate", "ninja", "samurai", "sensei", "shaolin", "fighter", "boxer", "wuxia",
            "immortal", "sage", "chi", "ki", "fist", "palm", "kung fu"],
    "glitch": ["glitch", "virus", "pixel", "digital", "internet", "meme", "hacker", "8-bit", "program", "ai", "creepypasta", "error"],
    "speed": ["speed", "speedster", "quick", "quicksilver", "swift", "zoom", "flash", "fast", "sonic", "turbo", "velocity", "rush", "dash"],
    "ink": ["cartoon", "toon", "looney", "acme", "slapstick", "animated"],
}

ROLE_WORDS = {
    "sneak": ["ninja", "assassin", "thief", "rogue", "spy", "stalker", "hunter", "shadow", "creeper", "cat", "burglar", "sniper", "hitman", "infiltrator"],
    "mage": FLAVOUR_WORDS["mystic"] + ["psychic", "telepath", "telekinetic", "mind"],
    "gunner": ["gun", "gunner", "gunslinger", "cowboy", "sheriff", "soldier", "sergeant", "marine", "trooper", "cop", "agent", "sniper",
               "bounty", "mercenary", "commando", "outlaw", "pistol", "rifle", "blaster", "shooter", "pirate", "deputy", "stormtrooper"],
    "brute": ["giant", "titan", "colossus", "brute", "ogre", "troll", "golem", "behemoth", "juggernaut", "hulk", "big", "tank", "brawler",
              "gorilla", "ape", "kong", "bear", "rhino", "bull", "mammoth", "wrestler", "strongman"],
    "knight": ["knight", "paladin", "warrior", "soldier", "gladiator", "spartan", "viking", "samurai", "guard", "guardian", "captain",
               "commander", "king", "queen", "prince", "princess", "lord", "lady", "general", "barbarian", "swordsman", "fencer"],
    "healer": ["healer", "medic", "nurse", "cleric", "priest", "priestess", "saint"],
    "time": ["time", "chrono", "chronos", "clock", "tick", "hourglass", "temporal", "doctor who", "tardis", "timekeeper"],
    "trick": ["joker", "jester", "clown", "trickster", "prankster", "loki", "imp", "gremlin", "harlequin", "puck", "goblin", "fool",
              "mischief", "madman", "mad"],
    "civilian": ["mom", "dad", "kid", "baby", "boy", "girl", "student", "reporter", "chef", "cook", "teacher", "farmer", "clerk",
                 "neighbor", "neighbour", "grandpa", "grandma", "uncle", "aunt", "intern", "manager", "boss", "janitor", "mailman"],
    "summoner": ["summoner", "queen", "king", "emperor", "empress", "lord", "master", "trainer", "warlord", "general", "commander",
                 "hive", "broodmother", "necromancer", "overlord", "chief", "leader"],
    "healing_factor": ["regenerat*", "healing factor", "wolverine", "deadpool", "immortal", "undying", "troll", "hydra"],
    "stretch": ["stretch", "elastic", "rubber", "plastic", "elongated", "mister fantastic", "mr fantastic", "gum"],
    "size": ["ant", "wasp", "giant man", "shrink", "size", "atom", "tiny", "micro", "yellowjacket"],
}

WEAPON_WORDS = {
    "sword": ["sword", "katana", "blade", "saber", "sabre", "cutlass", "rapier", "lightsaber", "longsword", "greatsword", "scimitar", "machete", "swordsman", "sword master", "swords"],
    "axe": ["axe", "hatchet", "cleaver", "battleaxe", "leviathan"],
    "hammer": ["hammer", "mace", "club", "maul", "mjolnir", "sledge", "sledgehammer", "flail", "morningstar", "baseball bat"],
    "spear": ["spear", "lance", "trident", "polearm", "halberd", "glaive", "naginata", "pike", "scythe", "javelin"],
    "bow": ["bow", "arrow", "archer", "crossbow", "quiver", "marksman", "hawkeye", "robin hood"],
    "gun": ["gun", "pistol", "rifle", "blaster", "revolver", "shotgun", "sniper", "cannon", "guns", "gunslinger", "laser", "minigun", "bazooka", "launcher", "raygun"],
    "whip": ["whip", "chain", "lasso", "rope", "flail", "tentacle", "kusarigama", "chainsaw"],
    "claws": ["claw", "claws", "talon", "talons", "wolverine", "cat", "tiger", "lion", "panther", "wolf", "werewolf", "bear", "raptor"],
    "staff": ["staff", "wand", "rod", "cane", "scepter", "sceptre", "bo", "stick", "broom"],
    "shield": ["shield", "buckler", "aegis"],
    "knives": ["knife", "knives", "dagger", "daggers", "kunai", "shuriken", "throwing", "stiletto", "sai"],
    "fists": ["fist", "fists", "gauntlet", "gauntlets", "knuckle", "boxer", "brawler", "punch", "puncher", "martial"],
}

COSTUME_WEAPON = {  # sources.COSTUME_WEAPON_ACC values → weapon class
    "sword": "sword", "greatsword": "sword", "twin swords": "sword", "katana": "sword", "lightsaber": "sword",
    "staff": "staff", "hammer": "hammer", "greathammer": "hammer", "axe": "axe", "polearm": "spear", "bow": "bow",
    "guns": "gun", "arm cannon": "gun", "claws": "claws", "gauntlet": "fists", "shield": "shield",
}

# Payload forms of an icon (what it fires / carries) → weapon class.
PAYLOAD_WEAPON = {"Sword": "sword", "Axe": "axe", "Hammer": "hammer", "Spear": "spear", "Bow": "bow", "Gun": "gun", "Cannon": "gun",
                  "Whip": "whip", "Staff": "staff", "Shield": "shield", "Disc": "knives", "Gauntlet": "fists"}

# Pack → impact-frame style for ultimates (assets/tuning/impact_frames.ron styles) and default flavour.
PACK_IMPACT = {"dc": "comic", "marvel": "comic", "tmnt": "comic", "anime": "ink", "internet": "glitch", "games": None,
               "shows": "comic", "film_tv": None, "pokemon": None, "transformers": None, "tabletop": None, "folklore_cn": "ink"}

# Franchise disambiguation suffixes in icon / names lists ("hogun marvel" → also "hogun").
STRIP_SUFFIXES = {"marvel", "dc", "transformer", "transformers", "tmnt", "autobot", "decepticon", "maximal", "predacon", "dinobot",
                  "invincible", "pokemon", "pixar", "dd", "disney", "apex", "valorant", "overwatch", "gow", "zelda", "mcu", "dceu",
                  "aerialbot", "stunticon", "constructicon", "insecticon", "combaticon", "protectobot", "terrorcon", "seacon",
                  "wreckers", "mcu", "comics", "ffvii", "ff7", "ff", "dbz", "naruto", "bleach", "jjk", "mha", "mk", "sf"}

# Pokémon moves by element (names only), used for the pokemon pack.
POKEMON = {
    "Fire": dict(bolt="flamethrower", storm="overheat", combo="fire spin", step="flame charge", ward="protect"),
    "Electric": dict(bolt="thunderbolt", storm="thunder", combo="volt tackle", step="volt switch", ward="protect"),
    "Water": dict(bolt="bubble beam", storm="hydro pump", combo="waterfall", step="aqua jet", ward="withdraw"),
    "Ice": dict(bolt="ice beam", storm="blizzard", combo="icicle crash", step="ice shard", ward="protect"),
    "Wind": dict(bolt="air slash", storm="hurricane", combo="aerial ace", step="fly", ward="roost"),
    "Earth": dict(bolt="rock slide", storm="earthquake", combo="rock smash", step="dig", ward="iron defense"),
    "Poison": dict(bolt="sludge bomb", storm="sludge wave", combo="poison jab", step="agility", ward="protect"),
    "Psychic": dict(bolt="psybeam", storm="future sight", combo="psycho cut", step="teleport", ward="reflect"),
    "Shadow": dict(bolt="shadow ball", storm="phantom force", combo="night slash", step="shadow sneak", ward="detect"),
    "Atomic": dict(bolt="signal beam", storm="hyper beam", combo="double edge", step="extreme speed", ward="protect"),
    "Light": dict(bolt="swift", storm="hyper beam", combo="fury swipes", step="agility", ward="protect"),
}
