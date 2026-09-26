"""Generates assets/icons/internet.ron. Usage: python tools/icons/internet/gen.py [out.ron]
Then: python tools/icons/internet/names.py  (writes assets/icons/internet_names.txt).
Review shapes: ICON_FILE=assets/icons/internet.ron cargo run --release --manifest-path tools/icons/internet/iconview/Cargo.toml -- --icons --out sheet.png
(iconview = scratch copy of bd_shapes' gallery example that reads IconEntry files; optional ICON_ONLY=id1,id2)."""
import sys
from pathlib import Path
ROOT = Path(__file__).resolve().parents[3]  # repo root when run from tools/icons/internet
from lib import E, R, emit, ENTRIES, scaled, check
import ishapes as sh

YEL = (1.0, 0.85, 0.15)
ORG = (1.0, 0.55, 0.1)
PNK = (1.0, 0.45, 0.7)
WHT = (0.95, 0.95, 0.95)
BRN = (0.55, 0.35, 0.2)
GRN = (0.35, 0.8, 0.2)
BLU = (0.25, 0.55, 1.0)
RED = (0.9, 0.15, 0.15)
GLD = (1.0, 0.8, 0.3)
GRY = (0.6, 0.6, 0.65)
PUR = (0.6, 0.3, 0.9)
CRM = (0.95, 0.85, 0.65)

BR = "Italian brainrot (AI meme, 2025)"

# =============================================================== ITALIAN BRAINROT (2025-26)
E(["tralalero tralala", "tralalero", "tralalelo tralala", "tralalero tralalá", "shark in nikes",
   "shark with nikes", "shark in sneakers", "three legged shark"], BR,
  R("Tralalero Tralala", "Fish", ["Walks", "Fast", "Charge"], "Water", 2.2, 120, accent=BLU,
    temp="Playful", inten=0.8), sh.tralalero(),
  "a sneaker shark that sprints and body-slams on its three legs")
E(["tung tung tung sahur", "tung tung sahur", "tung tung tung tung sahur", "tung sahur", "tuntuntun sahur",
   "tung tung", "sahur"], BR,
  R("Tung Tung Tung Sahur", "Biped", ["Blunt", "Charge"], "Earth", 2.3, 80, accent=BRN, temp="Sneaky", inten=0.9),
  sh.tung_sahur(), "the 3am log that sneaks up and swings its bat")
E(["bombardiro crocodilo", "bombardino crocodilo", "bombardilo crocodilo", "bombardiro", "bombardino",
   "crocodile bomber", "croc bomber plane"], BR,
  R("Bombardiro Crocodilo", "Bird", ["Flies", "Shoots"], "Fire", 4.0, 900, accent=GRN,
    payload=R("bomb", "Bomb", ["Explodes"], "Fire", 0.6, 20), feat=["Propeller"], temp="Aggressive", inten=0.8),
  sh.bombardiro(), "a crocodile bomber plane that carpet-bombs")
E(["bombombini gusini", "bombombini goosini", "goose bomber"], BR,
  R("Bombombini Gusini", "Bird", ["Flies", "Shoots"], "Fire", 3.0, 300,
    payload=R("bomb", "Bomb", ["Explodes"], "Fire", 0.5, 15), feat=["Jet"], temp="Chaotic", inten=0.9))
E(["cappuccino assassino", "capuccino assassino", "cappuccino assasino", "ninja coffee", "coffee ninja"], BR,
  R("Cappuccino Assassino", "Biped", ["Blade", "Fast"], "Fire", 1.2, 8, accent=BRN, temp="Sneaky", inten=1.0),
  sh.cappuccino(), "a coffee-cup ninja: twin katanas, flanks you")
E(["ballerina cappuccina", "ballerina capuchina", "ballerina cappucina"], BR,
  R("Ballerina Cappuccina", "Biped", ["Spin", "Fast"], "Light", 1.6, 45, accent=PNK, temp="Playful", inten=0.8),
  note="pirouettes through you")
E(["chimpanzini bananini", "chimpanzini", "chimpanzee banana", "banana monkey", "bananini"], BR,
  R("Chimpanzini Bananini", "Biped", ["Shoots", "Fast"], "Light", 1.2, 20, accent=YEL,
    payload=R("banana peel", "Ramp", ["Launch"], "Light", 0.5, 1), temp="Playful", inten=0.9),
  sh.chimpanzini(), "throws banana peels that launch whoever steps on them")
E(["brr brr patapim", "brr brr patapin", "patapim"], BR,
  R("Brr Brr Patapim", "Biped", ["Grab", "Stomp"], "Earth", 2.4, 150, accent=GRN, temp="Lazy", inten=0.5))
E(["lirili larila", "lirilì larilà", "cactus elephant"], BR,
  R("Lirili Larila", "Quadruped", ["Pierce", "Stomp"], "Earth", 3.0, 2000, feat=["Spikes"], temp="Calm"))
E(["trippi troppi", "trippi troppi troppa trippa", "shrimp cat"], BR,
  R("Trippi Troppi", "Fish", ["Grab", "Walks"], "Water", 1.2, 15, temp="Playful", inten=0.7))
E(["la vaca saturno saturnita", "vaca saturno", "saturnita", "saturn cow"], BR,
  R("La Vaca Saturno Saturnita", "Quadruped", ["Flies", "Attract", "Heavy"], "Psychic", 3.0, 3000,
    temp="Lazy", inten=0.6), note="a cow-planet whose gravity pulls everything in")
E(["frigo camelo", "fridge camel", "frigocamelo"], BR,
  R("Frigo Camelo", "Quadruped", ["Breath", "Stomp", "Heavy"], "Ice", 2.6, 900, temp="Calm"))
E(["bobrito bandito", "gangster beaver"], BR,
  R("Bobrito Bandito", "Biped", ["Shoots"], "Light", 1.3, 30, temp="Sneaky", inten=0.8))
E(["orcalero orcala", "orca in sneakers"], BR,
  R("Orcalero Orcala", "Fish", ["Walks", "Charge", "Heavy"], "Water", 3.5, 2500, temp="Aggressive", inten=0.7))
E(["boneca ambalabu", "ambalabu", "tire frog"], BR,
  R("Boneca Ambalabu", "Quadruped", ["Fast", "Charge"], "Earth", 1.4, 60, feat=["Wheels"], temp="Chaotic", inten=0.8))
E(["trulimero trulicina", "fish cat legs"], BR,
  R("Trulimero Trulicina", "Fish", ["Walks", "Grab"], "Water", 1.4, 25, temp="Playful", inten=0.6))
E(["girafa celestre", "giraffa celeste", "space giraffe"], BR,
  R("Girafa Celestre", "Quadruped", ["Flies", "Thrust"], "Light", 4.0, 800, temp="Playful", inten=0.6))
E(["ta ta ta sahur", "tatata sahur", "teapot legs"], BR,
  R("Ta Ta Ta Sahur", "Biped", ["Breath", "Fast"], "Water", 1.6, 20, temp="Chaotic", inten=0.7))
E(["glorbo fruttodrillo", "watermelon crocodile"], BR,
  R("Glorbo Fruttodrillo", "Quadruped", ["Grab", "Heavy"], "Water", 2.0, 400, temp="Lazy", inten=0.7))
E(["strawberry elephant", "fragola elefante"], "Steal a Brainrot (Roblox, 2025)",
  R("Strawberry Elephant", "Quadruped", ["Stomp", "Shoots"], "Light", 2.5, 1500, accent=RED,
    payload=R("strawberry", "Ball", ["Bounce"], "Light", 0.4, 1), temp="Playful", inten=0.6))

# =============================================================== SKIBIDI / BRAINROT-ADJACENT
E(["skibidi toilet", "skibidi", "skibidi toilets", "skbidi toilet", "skibbidi toilet", "skibity toilet"],
  "Skibidi Toilet (DaFuq!?Boom!, 2023)",
  R("Skibidi Toilet", "Blob", ["Grab", "Walks"], "Water", 1.6, 40, accent=WHT, temp="Chaotic", inten=0.9),
  sh.skibidi(), "a hopping toilet head that grabs and flushes you away")
E(["titan speakerman", "speakerman", "speaker man"], "Skibidi Toilet (DaFuq!?Boom!, 2023)",
  R("Titan Speakerman", "Kaiju", ["Repel", "Stomp"], "Wind", 14.0, 60000, feat=["Dish"], temp="Protective", inten=0.7),
  note="a giant loudspeaker: every step shockwaves, the bass blows you away")
E(["titan cameraman", "cameraman", "camera man"], "Skibidi Toilet (DaFuq!?Boom!, 2023)",
  R("Titan Cameraman", "Kaiju", ["Beam", "Stomp"], "Light", 14.0, 60000, feat=["Headlight"], temp="Aggressive", inten=0.7))
E(["titan tv man", "tv man", "tvman"], "Skibidi Toilet (DaFuq!?Boom!, 2023)",
  R("Titan TV Man", "Kaiju", ["Beam", "Stomp"], "Psychic", 14.0, 60000, feat=["Windows"], temp="Calm"))

# =============================================================== VIRAL CREATURES & ANIMALS
E(["labubu", "labubus", "la bu bu", "lububu", "labooboo", "lafufu"], "The Monsters (Kasing Lung / Pop Mart)",
  R("Labubu", "Biped", ["Sticky", "Swarm"], "Light", 0.9, 3, count=3, accent=CRM, temp="Chaotic", inten=0.9),
  sh.labubu(), "a blind-box swarm of grinning bag charms that cling to you")
E(["crybaby", "crybaby doll", "crybaby pop mart"], "Crybaby (Molly/Pop Mart designer toy)",
  R("Crybaby", "Biped", ["Breath"], "Water", 0.9, 3, temp="Fearful", inten=0.8), note="sprays a cone of tears")
E(["moo deng", "moodeng", "moo-deng", "bouncy pig", "baby pygmy hippo", "sassy hippo"],
  "Moo Deng (Khao Kheow Open Zoo, 2024)",
  R("Moo Deng", "Quadruped", ["Charge", "Grab"], "Water", 1.0, 40, accent=PNK, temp="Aggressive", inten=0.9),
  sh.moo_deng(), "the tiny hippo that charges and chomps your ankles")
E(["haggis", "haggis the hippo"], "Haggis (Edinburgh Zoo, 2024)",
  R("Haggis", "Quadruped", ["Charge", "Bounce"], "Water", 1.0, 40, temp="Playful", inten=0.8))
E(["pesto", "pesto the penguin", "pesto penguin", "big baby penguin", "fat penguin"],
  "Pesto (Sea Life Melbourne, 2024)",
  R("Pesto", "Biped", ["Heavy", "Bounce"], "Ice", 1.2, 25, accent=ORG, temp="Lazy", inten=0.8),
  sh.pesto(), "a gigantic fluffy chick: a bouncy, lazy roadblock")
E(["nihilistic penguin", "herzog penguin", "penguin walking to the mountains", "that penguin", "the penguin going to the mountains"],
  "Encounters at the End of the World (Herzog) meme, 2026",
  R("Nihilistic Penguin", "Biped", ["Walks"], "Ice", 0.7, 5, temp="Fearful", inten=1.0),
  note="walks away from everything, including the fight")
E(["chill guy", "my new character is a chill guy", "chillguy", "just a chill guy"], "Chill Guy (Phillip Banks, 2023-24)",
  R("Chill Guy", "Biped", ["Guard", "Heavy"], "Light", 1.8, 70, accent=(0.8, 0.65, 0.5), temp="Lazy", inten=1.0),
  sh.chill_guy(), "hands in pockets, blocks everything, cannot be bothered to move")
E(["pedro", "pedro raccoon", "raccoon pedro", "spinning raccoon", "dancing raccoon", "pedro pedro pedro"],
  "Pedro raccoon meme (2024)",
  R("Pedro", "Quadruped", ["Spin", "Fast"], "Light", 0.9, 10, accent=GRY, temp="Playful", inten=1.0),
  sh.pedro(), "a raccoon spinning to the beat, contact damage all round")
E(["jimothy", "jimothy the raccoon", "round raccoon", "ball raccoon", "round boy"], "Jimothy (Seattle, 2026)",
  R("Jimothy", "Boulder", ["Bounce"], "Light", 1.0, 30, accent=GRY), sh.jimothy(),
  "the spherical Seattle raccoon: roll him at people")
E(["punch the monkey", "punch monkey", "hang in there punch", "baby macaque"], "Punch (Ichikawa City Zoo, 2026)",
  R("Punch", "Biped", ["Grab", "Sticky"], "Light", 0.6, 3, temp="Protective", inten=0.9),
  note="clings to you like his plushie and guards you")
E(["capybara", "capy", "capybaras", "kapibara", "capybara with yuzu", "ok i pull up"], "Capybara memes",
  R("Capybara", "Quadruped", ["Barrier", "Heavy"], "Water", 1.2, 55, accent=ORG, temp="Lazy", inten=1.0),
  sh.capybara(), "the chillest roadblock: sits in the way, nothing bothers it")
E(["doge", "shiba doge", "much wow", "doge dog", "kabosu"], "Doge (Kabosu, 2010-13)",
  R("Doge", "Quadruped", ["Charge", "Bounce"], "Light", 0.9, 12, accent=CRM, temp="Playful", inten=0.8),
  sh.doge(), "such charge, very bounce, wow")
E(["cheems", "cheemsburger", "cheems doge"], "Cheems (Balltze) meme",
  R("Cheems", "Quadruped", ["Bounce"], "Light", 0.9, 12, temp="Fearful", inten=0.8))
E(["bonk", "bonk bat", "bonk hammer", "bonked"], "Cheems 'bonk' meme",
  R("Bonk", "Hammer", ["Blunt", "Bounce"], "Light", 1.0, 5, accent=YEL), note="cartoon bonk: sends them bouncing")
E(["nyan cat", "nyancat", "poptart cat", "pop tart cat", "rainbow cat"], "Nyan Cat (prguitarman, 2011)",
  R("Nyan Cat", "Bird", ["Flies", "Fast", "Homing"], "Light", 1.2, 5, accent=PNK, temp="Playful", inten=1.0),
  sh.nyan_cat(), "flies straight at you trailing a rainbow")
E(["longcat", "long cat", "longcat is long"], "Longcat (2006)",
  R("Longcat", "Serpent", ["Constrict"], "Light", 4.0, 20, temp="Lazy", inten=0.7), note="is long; wraps you")
E(["grumpy cat", "tardar sauce", "grumpycat"], "Grumpy Cat (2012)",
  R("Grumpy Cat", "Quadruped", ["Repel"], "Shadow", 0.6, 4, temp="Lazy", inten=1.0), note="'no.' - pushes everyone away")
E(["maxwell", "maxwell the cat", "spinning cat"], "Maxwell the Cat (2022)",
  R("Maxwell the Cat", "Quadruped", ["Spin"], "Light", 0.7, 6, temp="Playful", inten=0.9))
E(["oiia cat", "oiiaioiiiai cat", "oiia oiia", "spinning oiia cat", "oia cat"], "OIIA cat (2024)",
  R("OIIA Cat", "Quadruped", ["Spin", "Fast", "Repel"], "Wind", 0.7, 6, temp="Chaotic", inten=1.0))
E(["big floppa", "floppa", "caracal floppa"], "Big Floppa (2017-20)",
  R("Big Floppa", "Quadruped", ["Heavy", "Charge"], "Light", 1.1, 18, temp="Lazy", inten=0.6))
E(["chipi chipi chapa chapa", "chipi chipi cat", "dubi dubi daba daba"], "Chipi chipi cat (2023)",
  R("Chipi Chipi Cat", "Quadruped", ["Bounce", "Fast"], "Light", 0.6, 4, temp="Playful", inten=1.0))
E(["banana cat", "crying banana cat", "sad banana cat"], "Banana cat (2023)",
  R("Banana Cat", "Quadruped", ["Breath"], "Water", 0.6, 4, accent=YEL, temp="Fearful", inten=0.9))
E(["gigachad", "giga chad", "chad", "sigma male", "sigma"], "Gigachad (2017)",
  R("Gigachad", "Biped", ["Heavy", "Stomp", "Grab"], "Light", 2.2, 110, temp="Protective", inten=0.9),
  sh.gigachad(), "the jawline stomps, grabs and throws")
E(["amogus", "among us", "amongus", "among us crewmate", "crewmate", "impostor", "imposter", "sus", "amogus sus"],
  "Among Us (Innersloth, 2018)",
  R("Amogus", "Biped", ["Blade", "Fast"], "Light", 1.1, 20, accent=(0.6, 0.85, 1.0), temp="Sneaky", inten=1.0),
  sh.amogus(), "an impostor: sneaks up behind and stabs")
E(["shrek", "ogre", "swamp ogre", "shrek the ogre", "get out of my swamp"], "Shrek (DreamWorks, 2001)",
  R("Shrek", "Biped", ["Repel", "Stomp", "Heavy"], "Poison", 2.3, 160, accent=(0.9, 0.85, 0.7), temp="Aggressive", inten=0.6),
  sh.shrek(), "roars you out of his swamp and stomps")
E(["big chungus", "chungus"], "Big Chungus meme (2018)",
  R("Big Chungus", "Biped", ["Heavy", "Bounce", "Stomp"], "Light", 2.5, 300, temp="Lazy", inten=0.8))
E(["trollface", "troll face", "problem?"], "Trollface (Whynne, 2008)",
  R("Trollface", "Blob", ["Homing", "Repel"], "Light", 1.2, 5, temp="Chaotic", inten=1.0))
E(["grimace shake", "grimace", "grimace milkshake"], "Grimace Shake trend (2023)",
  R("Grimace Shake", "Bomb", ["Explodes", "Sticky"], "Poison", 0.5, 2, accent=PUR))
E(["gorilla vs 100 men", "silverback", "one gorilla", "1 gorilla"], "'100 men vs 1 gorilla' debate (2025)",
  R("Silverback Gorilla", "Biped", ["Grab", "Stomp", "Heavy"], "Earth", 2.0, 200, temp="Aggressive", inten=0.8),
  sh.gorilla(), "the gorilla from the 100-men debate: grabs and throws")
E(["100 men", "hundred men", "100 guys", "one hundred men"], "'100 men vs 1 gorilla' debate (2025)",
  R("100 Men", "Biped", ["Swarm", "Charge"], "Light", 1.8, 75, count=100, temp="Aggressive", inten=0.7),
  note="count clamps, but they will try")
E(["six seven", "6 7", "67", "6-7", "sixseven"], "'6-7' meme (2025)",
  R("6-7", "Ball", ["Swarm", "Bounce"], "Light", 0.3, 1, count=67, temp="Chaotic", inten=1.0),
  note="67 tiny bouncing orbs (the sim clamps the count)")
E(["crab rave", "crab rave crabs", "dancing crabs"], "Crab Rave (Noisestorm, 2018)",
  R("Crab Rave", "Arachnid", ["Swarm", "Grab"], "Water", 0.5, 2, count=8, temp="Playful", inten=1.0))
E(["untitled goose", "evil goose", "honk", "peace was never an option", "the goose"], "Untitled Goose Game (House House, 2019)",
  R("Untitled Goose", "Bird", ["Grab", "Charge", "Flies"], "Light", 1.0, 5, temp="Chaotic", inten=1.0),
  note="steals whatever it grabs and runs off honking")
E(["chicken jockey", "chicken jockey!"], "A Minecraft Movie (2025)",
  R("Chicken Jockey", "Bird", ["Charge", "Flies"], "Light", 1.2, 8, temp="Chaotic", inten=1.0))
E(["skeleton banging shield", "skeleton shield", "banging shield skeleton"], "Skeleton Banging Shield (2025)",
  R("Shield-Banging Skeleton", "Biped", ["Guard", "Repel"], "Shadow", 1.8, 20, temp="Aggressive", inten=0.7))
E(["this is fine", "this is fine dog", "everything is fine"], "On Fire (KC Green, 2013)",
  R("This Is Fine", "Dome", ["Barrier"], "Fire", 4.0, 100), note="a burning room around you. it's fine")
E(["stonks", "stonks man", "line go up", "number go up"], "Stonks meme (2017)",
  R("Stonks", "Ramp", ["Launch", "Fast"], "Light", 4.0, 100, accent=GRN), note="line goes up, so do you")
E(["the rock", "rock eyebrow", "sus rock", "rock with eyebrow", "vine boom rock"], "'The Rock eyebrow' reaction meme",
  R("The Rock", "Boulder", ["Heavy"], "Earth", 1.6, 800, temp="Calm"))
E(["vine boom", "vine boom sound", "vine thud"], "Vine boom sound effect",
  R("Vine Boom", "Bomb", ["Explodes", "Repel"], "Wind", 0.8, 5))
E(["moai", "moyai", "🗿", "easter island head", "moai head", "stone face emoji"], "Moai (Rapa Nui) / 🗿 emoji",
  R("Moai", "Pillar", ["DropFromSky", "Heavy", "Barrier"], "Earth", 2.3, 8000, accent=RED),
  sh.moai(), "falls from the sky with a vine boom, then stands there judging you")
E(["rickroll", "rick roll", "rickrolled", "rick rolled"], "Rickroll (2007)",
  R("Rickroll", "Cage", ["Trap", "Homing"], "Psychic", 2.0, 60, temp="Playful", inten=1.0),
  note="homing trap: you knew the rules")
E(["backrooms", "the backrooms", "backrooms wall", "level 0"], "The Backrooms (2019; film 2026)",
  R("The Backrooms", "Wall", ["Portal", "Barrier"], "Shadow", 4.0, 400, accent=(0.9, 0.85, 0.45)),
  note="a yellow wall that noclips you somewhere else")
E(["404", "error 404", "404 not found"], "HTTP 404",
  R("404 Not Found", "Blob0", ["Sticky"], "Psychic", 1.0, 10), note="deliberately the unknown blob")
E(["blue screen of death", "bsod", "blue screen"], "Windows BSOD",
  R("Blue Screen of Death", "Wall", ["Barrier", "Repel"], "Electric", 5.0, 400, accent=BLU))
E(["bliss", "windows xp hill", "bliss hill", "windows xp wallpaper"], "Bliss (Charles O'Rear, Windows XP)",
  R("Bliss Hill", "Ramp", ["Launch"], "Earth", 6.0, 1000, accent=GRN))
E(["gnome", "garden gnome", "you've been gnomed", "gnomed", "gnomes"], "'You've been gnomed' meme",
  R("Garden Gnome", "Biped", ["Swarm", "Charge"], "Earth", 0.6, 4, count=4, accent=RED, temp="Sneaky", inten=0.9))
E(["flying spaghetti monster", "fsm", "spaghetti monster"], "Flying Spaghetti Monster (2005)",
  R("Flying Spaghetti Monster", "Blob", ["Flies", "Constrict", "Grab"], "Psychic", 3.0, 80, temp="Calm"))
E(["baked potato with ski goggles", "ski potato", "potato with goggles"], "Ski-goggle potato meme (2026)",
  R("Ski Potato", "Boulder", ["Fast"], "Ice", 0.8, 5))
E(["viking row", "viking clap", "viking boat row"], "Viking Row chant (2026 World Cup)",
  R("Viking Row Longship", "Vehicle", ["Piloted", "Charge", "Fast"], "Water", 8.0, 3000, feat=["Sails"],
    temp="Aggressive", inten=0.8), note="the whole stand rows you into the enemy")

# =============================================================== TECH MASCOTS
E(["duolingo owl", "duo", "duo the owl", "duolingo", "duo owl", "the green owl"], "Duolingo mascot",
  R("Duo the Owl", "Bird", ["Flies", "Homing", "Fast"], "Psychic", 1.4, 10, accent=ORG, temp="Aggressive", inten=1.0),
  sh.duolingo(), "you missed your lesson. it will find you")
E(["clippy", "clippit", "mr clippy", "microsoft clippy", "paperclip assistant"], "Microsoft Office Assistant (1997)",
  R("Clippy", "Serpent", ["Homing", "Link"], "Electric", 2.2, 5, temp="Playful", inten=1.0),
  sh.clippy(), "it looks like you're trying to escape. would you like to be chained to something?")
E(["bonzi buddy", "bonzibuddy", "bonzi"], "BonziBuddy (1999)",
  R("BonziBuddy", "Biped", ["Swarm", "Homing"], "Psychic", 1.5, 40, count=3, accent=PUR, temp="Sneaky", inten=0.8))
E(["ferris", "ferris the crab", "rust crab", "rustacean", "rust mascot"], "Rust language mascot",
  R("Ferris", "Arachnid", ["Grab", "Guard"], "Fire", 1.3, 10, accent=(0.95, 0.45, 0.15), temp="Protective", inten=0.8),
  sh.ferris(), "memory-safe: guards its owner and borrows your legs")
E(["tux", "linux penguin", "tux penguin"], "Linux mascot (Larry Ewing, 1996)",
  R("Tux", "Biped", ["Guard", "Bounce"], "Ice", 1.0, 20, accent=YEL, temp="Protective", inten=0.7))
E(["octocat", "github cat", "github octocat"], "GitHub mascot",
  R("Octocat", "Quadruped", ["Grab", "Constrict"], "Light", 1.0, 10, temp="Playful", inten=0.6))
E(["gopher", "go gopher", "golang gopher"], "Go language mascot (Renee French)",
  R("Go Gopher", "Biped", ["Fast", "Swarm"], "Earth", 0.8, 5, count=4, accent=(0.5, 0.85, 1.0), temp="Playful", inten=0.8),
  note="concurrent: many goroutines at once")
E(["docker whale", "moby dock", "docker"], "Docker mascot",
  R("Moby Dock", "Fish", ["Flies", "Shoots"], "Water", 4.0, 3000, accent=BLU,
    payload=R("shipping container", "Anvil", ["DropFromSky", "Heavy"], "Light", 2.0, 2000), temp="Calm"),
  note="ships containers - onto your head")
E(["chrome dino", "t-rex runner", "no internet dinosaur", "dino game", "offline dinosaur", "chrome dinosaur"],
  "Chrome offline game (Google, 2014)",
  R("Chrome Dino", "Biped", ["Fast", "Bounce"], "Light", 1.6, 30, accent=GRY, temp="Playful", inten=0.6),
  sh.chrome_dino(), "runs and jumps; pixel-perfect")
E(["snoo", "reddit alien", "reddit snoo"], "Reddit mascot",
  R("Snoo", "Biped", ["Beam"], "Light", 1.2, 8, feat=["Antenna"], temp="Playful", inten=0.6))
E(["firefox", "firefox fox"], "Mozilla Firefox logo",
  R("Firefox", "Quadruped", ["Fast", "Charge", "Constrict"], "Fire", 1.2, 15, accent=ORG, temp="Aggressive", inten=0.7),
  note="a fox of fire that wraps around the globe (you)")
E(["android robot", "bugdroid", "android mascot"], "Android mascot",
  R("Bugdroid", "Biped", ["Shoots"], "Electric", 1.2, 30, accent=GRN, feat=["Antenna"], temp="Calm"))
E(["roomba", "robot vacuum", "roomba cat", "cat on a roomba"], "Roomba (iRobot) / cat-on-roomba videos",
  R("Roomba", "Vehicle", ["Attract", "Homing", "Piloted"], "Wind", 0.9, 4, feat=["Wheels"], temp="Chaotic", inten=0.5),
  sh.roomba(), "vacuums debris and players toward it; a cat pilots it")
E(["nokia 3310", "nokia", "nokia brick", "indestructible nokia"], "Nokia 3310 meme (indestructible phone)",
  R("Nokia 3310", "Anvil", ["DropFromSky", "Heavy"], "Earth", 1.5, 5000),
  sh.nokia(), "the indestructible phone, dropped from orbit")
E(["floppy disk", "save icon", "floppy"], "3.5-inch floppy disk",
  R("Floppy Disk", "Disc", ["Ricochet"], "Electric", 0.8, 1))

# =============================================================== TOYS & COLLECTIBLES
E(["rubber duck", "rubber ducky", "rubber duckie", "debugging duck", "rubber duck debugging", "ducky"],
  "Rubber duck (toy)",
  R("Rubber Duck", "Ball", ["Bounce", "Ricochet"], "Water", 1.0, 2, accent=YEL), sh.rubber_duck(),
  "squeak: bounces off everything")
E(["lego", "lego brick", "legos", "lego bricks", "stepping on lego", "step on a lego"], "LEGO brick",
  R("LEGO Brick", "Ball", ["Pierce", "Sticky", "Swarm"], "Light", 0.5, 1, count=6, accent=RED),
  sh.lego(), "scatters bricks that stick where they land and hurt to step on")
E(["rubik's cube", "rubiks cube", "rubix cube", "rubik cube", "magic cube", "speedcube"], "Rubik's Cube (Ernő Rubik, 1974)",
  R("Rubik's Cube", "Ball", ["Spin", "Ricochet"], "Psychic", 2.2, 30, accent=YEL), sh.rubiks(),
  "a spinning cube that ricochets between targets")
E(["beyblade", "beyblades", "let it rip", "battle top"], "Beyblade (Takara Tomy)",
  R("Beyblade", "Disc", ["Spin", "Ricochet", "Fast"], "Wind", 1.8, 4, accent=BLU), sh.beyblade(),
  "let it rip: spins, grinds and ricochets")
E(["yo-yo", "yoyo", "yo yo"], "Yo-yo",
  R("Yo-yo", "Disc", ["Returning", "Pull"], "Light", 1.0, 2, accent=RED), sh.yoyo(),
  "flicks out, snags you, reels you back")
E(["fidget spinner", "spinner", "fidget spinners"], "Fidget spinner (2017 craze)",
  R("Fidget Spinner", "Disc", ["Spin", "Blade"], "Light", 1.4, 2, accent=BLU), sh.fidget_spinner(),
  "a saw-blade of boredom")
E(["nerf gun", "nerf", "nerf blaster", "nerf dart"], "Nerf (Hasbro)",
  R("Nerf Blaster", "Gun", ["Shoots", "Sticky"], "Light", 0.8, 2, accent=ORG), note="suction darts stick to you")
E(["furby", "furbies", "furby boom"], "Furby (Tiger Electronics, 1998)",
  R("Furby", "Biped", ["Swarm", "Homing"], "Psychic", 0.5, 1, count=3, temp="Chaotic", inten=0.9),
  note="three of them, awake, at night")
E(["tamagotchi", "tamagochi", "virtual pet"], "Tamagotchi (Bandai, 1996)",
  R("Tamagotchi", "Blob", ["Grow"], "Light", 0.8, 1, temp="Playful", inten=0.6), note="grows whatever it touches")
E(["slinky", "slinkie"], "Slinky (1945)",
  R("Slinky", "Whip", ["Pull", "Bounce"], "Light", 2.0, 1, accent=GRY))
E(["hot wheels", "hotwheels", "toy car", "matchbox car"], "Hot Wheels (Mattel)",
  R("Hot Wheels Car", "Vehicle", ["Fast", "Swarm", "Charge"], "Fire", 0.4, 1, count=4, feat=["Wheels", "Exhaust"]))
E(["hot wheels loop", "loop track", "orange track"], "Hot Wheels track",
  R("Hot Wheels Loop", "Ramp", ["Launch", "Fast"], "Light", 4.0, 50, accent=ORG))
E(["teddy bear", "teddy", "teddybear", "plushie", "stuffed animal"], "Teddy bear",
  R("Teddy Bear", "Biped", ["Guard", "Bounce"], "Light", 1.2, 3, accent=BRN, temp="Protective", inten=1.0))
E(["pop it", "pop-it", "popit", "pop it fidget"], "Pop It fidget toy",
  R("Pop It", "Trampoline", ["Bounce"], "Light", 3.0, 20, accent=PNK))
E(["needoh", "nee doh", "needoh squishy", "squishy", "stress ball"], "NeeDoh (Schylling) - 2026 sell-out craze",
  R("NeeDoh", "Ball", ["Bounce", "Sticky"], "Light", 0.8, 1, accent=PNK))
E(["bouncy ball", "super ball", "superball", "rubber ball"], "Super Ball (Wham-O)",
  R("Super Ball", "Ball", ["Bounce", "Ricochet", "Fast"], "Light", 0.5, 1))
E(["water balloon", "water bomb", "water balloons"], "Water balloon",
  R("Water Balloon", "Bomb", ["Explodes"], "Water", 0.5, 1, count=3))
E(["super soaker", "water gun", "squirt gun", "supersoaker"], "Super Soaker (Larami/Hasbro)",
  R("Super Soaker", "Gun", ["Beam", "Repel"], "Water", 1.0, 2, accent=ORG))
E(["silly string", "spray string"], "Silly String",
  R("Silly String", "Gun", ["Beam", "Sticky"], "Light", 0.5, 1, accent=PNK), note="a sticky beam")
E(["magic 8 ball", "magic eight ball", "8 ball", "eight ball"], "Magic 8 Ball (Mattel)",
  R("Magic 8 Ball", "Ball", ["Ricochet", "Homing"], "Psychic", 0.8, 3), note="'signs point to yes' - it finds you")
E(["hungry hungry hippos", "hungry hippos", "hungry hungry hippo"], "Hungry Hungry Hippos (Hasbro)",
  R("Hungry Hungry Hippos", "Quadruped", ["Grab", "Swarm", "Charge"], "Water", 1.0, 15, count=4, accent=GRN,
    temp="Aggressive", inten=1.0), sh.hungry_hippo(), "four chomping hippos")
E(["stretch armstrong", "stretchy man toy"], "Stretch Armstrong (Kenner)",
  R("Stretch Armstrong", "Biped", ["Grab", "Pull"], "Light", 1.8, 40, temp="Playful", inten=0.6))
E(["gummy bear", "gummy bears", "gummi bear", "gummibär"], "Gummy bear",
  R("Gummy Bear", "Biped", ["Sticky", "Bounce", "Swarm"], "Light", 0.5, 1, count=4, accent=RED, temp="Playful", inten=0.8))
E(["jenga", "jenga tower"], "Jenga (Hasbro)",
  R("Jenga Tower", "Pillar", ["Barrier", "DropFromSky"], "Earth", 4.0, 200, accent=CRM))
E(["jack in the box", "jack-in-the-box"], "Jack-in-the-box",
  R("Jack-in-the-box", "Bomb", ["Launch", "Explodes"], "Light", 0.8, 5, temp="Playful", inten=1.0))
E(["pinata", "piñata"], "Piñata",
  R("Piñata", "Quadruped", ["Explodes"], "Light", 1.4, 5, accent=PNK,
    payload=R("candy", "Ball", ["Swarm", "Bounce"], "Light", 0.2, 1, count=8), temp="Playful", inten=0.8))
E(["pogo stick", "pogo"], "Pogo stick",
  R("Pogo Stick", "Staff", ["Bounce", "Launch"], "Light", 1.4, 5))
E(["paper airplane", "paper plane"], "Paper airplane",
  R("Paper Airplane", "Bird", ["Flies", "Homing", "Glide"], "Wind", 0.5, 1, temp="Calm"))
E(["hamster ball", "zorb", "zorb ball", "human hamster ball"], "Zorbing",
  R("Hamster Ball", "Ball", ["Piloted", "Bounce"], "Light", 3.0, 40), note="get inside and roll")
E(["bouncy castle", "bounce house", "inflatable castle"], "Bouncy castle",
  R("Bouncy Castle", "Trampoline", ["Bounce", "Barrier"], "Light", 5.0, 100, accent=RED))
E(["whoopee cushion", "whoopie cushion", "fart cushion"], "Whoopee cushion",
  R("Whoopee Cushion", "Trampoline", ["Repel", "Bounce"], "Wind", 1.2, 1, accent=PNK), note="sit on it: pfffft, everyone flies")
E(["d20", "twenty sided die", "nat 20", "natural 20", "dice"], "D&D d20",
  R("d20", "Ball", ["Bounce", "Ricochet"], "Psychic", 0.8, 2, accent=RED))
E(["mousetrap", "mouse trap"], "Spring mousetrap",
  R("Mousetrap", "Cage", ["Trap", "Blunt"], "Light", 1.2, 3))

# =============================================================== SPORTS
E(["bowling ball", "bowling"], "Bowling",
  R("Bowling Ball", "Boulder", ["Heavy", "Fast"], "Earth", 1.0, 120, accent=PUR), sh.bowling_ball(),
  "strike: rolls fast and flattens")
E(["bowling pins", "bowling pin", "ten pins"], "Bowling",
  R("Bowling Pins", "Pillar", ["Barrier", "Swarm"], "Light", 1.2, 3, count=10, accent=RED))
E(["curling stone", "curling rock", "curling"], "Curling",
  R("Curling Stone", "Boulder", ["Heavy", "Fast"], "Ice", 1.1, 150, accent=RED), sh.curling_stone(),
  "slides across the map on ice")
E(["frying pan", "pan", "pubg pan", "skillet", "cast iron pan", "frypan"], "Frying pan (PUBG / cartoon slapstick)",
  R("Frying Pan", "Hammer", ["Blunt", "Guard"], "Fire", 1.3, 4, accent=YEL), sh.frying_pan(),
  "bonk and block bullets, like PUBG")
E(["cricket bat", "willow bat"], "Cricket",
  R("Cricket Bat", "Hammer", ["Blunt", "Guard"], "Light", 1.0, 3, accent=CRM), note="flat enough to block")
E(["hockey stick", "ice hockey stick"], "Ice hockey",
  R("Hockey Stick", "Staff", ["Blunt", "Shoots"], "Ice", 1.6, 2,
    payload=R("hockey puck", "Disc", ["Ricochet", "Fast"], "Ice", 0.3, 1)), note="slapshots ricocheting pucks")
E(["boxing glove", "boxing gloves"], "Boxing",
  R("Boxing Glove", "Gauntlet", ["Blunt", "Fast"], "Light", 0.6, 1, accent=RED))
E(["spring boxing glove", "punching glove trap", "boxing glove on a spring", "punching wall"], "Cartoon gag",
  R("Spring Glove", "Wall", ["Launch", "Blunt"], "Light", 1.5, 5, accent=RED), note="a wall that punches you across the map")
E(["basketball", "b-ball", "swish"], "Basketball",
  R("Basketball", "Ball", ["Bounce", "Homing"], "Light", 0.6, 1, accent=ORG), note="nothing but net")
E(["golf club", "driver", "fore"], "Golf",
  R("Golf Club", "Staff", ["Blunt", "Shoots"], "Light", 1.1, 1,
    payload=R("golf ball", "Ball", ["Fast", "Ricochet"], "Light", 0.2, 1)))
E(["soccer ball", "football ball", "world cup ball"], "Association football",
  R("Soccer Ball", "Ball", ["Bounce", "Fast"], "Light", 0.5, 1, accent=WHT))
E(["world cup trophy", "fifa world cup trophy", "world cup"], "FIFA World Cup (2026)",
  R("World Cup Trophy", "Hammer", ["Blunt", "Heavy"], "Light", 1.0, 6, accent=GLD))
E(["red card", "yellow card", "referee card"], "Football referee",
  R("Red Card", "Disc", ["Launch", "Homing"], "Light", 0.4, 1, accent=RED), note="you're sent off - literally")
E(["vuvuzela", "vuvuzelas"], "2010 World Cup",
  R("Vuvuzela", "Gun", ["Beam", "Repel"], "Wind", 1.0, 1, accent=YEL), note="a continuous BZZZZ that blows people away")
E(["air horn", "airhorn", "mlg air horn"], "MLG montage memes",
  R("Air Horn", "Gun", ["Repel"], "Wind", 0.5, 1))
E(["t-shirt cannon", "tshirt cannon", "shirt cannon"], "Stadium promo",
  R("T-shirt Cannon", "Cannon", ["Shoots"], "Light", 1.5, 10,
    payload=R("t-shirt", "Parachute", ["SlowFall", "Sticky"], "Light", 1.0, 1)), note="t-shirts drift down and stick")
E(["zamboni", "ice resurfacer"], "Zamboni",
  R("Zamboni", "Vehicle", ["Piloted", "Heavy"], "Ice", 3.5, 3000, feat=["Wheels", "Cab"]))
E(["pool noodle", "swim noodle"], "Pool noodle",
  R("Pool Noodle", "Staff", ["Bounce"], "Water", 1.6, 1, accent=PNK), note="harmless... but very bouncy")
E(["shot put", "shotput"], "Athletics",
  R("Shot Put", "Ball", ["Heavy", "Blunt"], "Earth", 0.4, 7))

# =============================================================== EVERYDAY FUNNY OBJECTS
E(["banana peel", "banana skin", "slippery banana", "banana peels"], "Slapstick",
  R("Banana Peel", "Ramp", ["Launch"], "Light", 0.8, 1, count=3, accent=YEL), sh.banana_peel(),
  "step on it and fly")
E(["traffic cone", "road cone", "traffic cones", "safety cone"], "Traffic cone",
  R("Traffic Cone", "Pillar", ["Barrier"], "Light", 1.1, 5, count=4, accent=ORG), sh.traffic_cone(),
  "four cones: road closed")
E(["shopping cart", "shopping trolley", "grocery cart"], "Shopping cart",
  R("Shopping Cart", "Vehicle", ["Charge", "Piloted", "Fast"], "Light", 1.2, 20, accent=RED, feat=["Wheels"]),
  sh.shopping_cart(), "hop in and ram people")
E(["office chair", "swivel chair", "spinny chair"], "Office chair",
  R("Office Chair", "Vehicle", ["Spin", "Piloted"], "Light", 1.2, 15, feat=["Wheels"]))
E(["grand piano", "piano", "falling piano", "baby grand"], "Cartoon slapstick",
  R("Grand Piano", "Anvil", ["DropFromSky", "Heavy"], "Light", 2.2, 500, accent=WHT), sh.grand_piano(),
  "the classic falling piano")
E(["acme anvil", "cartoon anvil", "falling anvil"], "Looney Tunes slapstick",
  R("ACME Anvil", "Anvil", ["DropFromSky", "Heavy"], "Light", 1.0, 1000))
E(["kitchen sink", "everything but the kitchen sink", "sink"], "Idiom",
  R("Kitchen Sink", "Anvil", ["DropFromSky", "Explodes"], "Water", 1.2, 150), note="drops, then bursts a pipe")
E(["vending machine", "soda machine"], "Vending machine",
  R("Vending Machine", "Wall", ["Barrier", "Shoots"], "Electric", 2.0, 400,
    payload=R("soda can", "Bomb", ["Explodes", "Fast"], "Water", 0.3, 1)), note="dispenses exploding soda")
E(["rubber chicken", "squeaky chicken"], "Gag toy",
  R("Rubber Chicken", "Hammer", ["Blunt", "Bounce"], "Light", 1.0, 1, accent=YEL))
E(["chancla", "la chancla", "flip flop", "flip-flop", "slipper", "tsinelas", "sandal", "mom's slipper"],
  "La chancla (the mum's slipper)",
  R("La Chancla", "Disc", ["Homing", "Returning", "Blunt"], "Light", 1.2, 1, accent=PNK), sh.chancla(),
  "never misses, always comes back")
E(["plunger", "toilet plunger"], "Plunger",
  R("Plunger", "Spear", ["Sticky", "Pull"], "Water", 1.2, 2), note="thrown, sticks, yanks you in")
E(["toilet paper", "tp", "toilet roll", "loo roll"], "Toilet paper",
  R("Toilet Paper", "Whip", ["Constrict", "Link"], "Light", 3.0, 1, accent=WHT), note="mummify and tie people up")
E(["leaf blower"], "Leaf blower",
  R("Leaf Blower", "Gun", ["Beam", "Repel"], "Wind", 1.0, 5))
E(["fire extinguisher", "extinguisher"], "Fire extinguisher",
  R("Fire Extinguisher", "Gun", ["Breath", "Thrust"], "Ice", 0.8, 8), note="spray foam or use it as a jetpack")
E(["cardboard box", "hide in a box"], "Stealth-game box",
  R("Cardboard Box", "Dome", ["Barrier"], "Light", 1.5, 2, accent=BRN), note="hide")
E(["wet floor sign", "caution wet floor", "wet floor"], "Janitorial",
  R("Wet Floor Sign", "Ramp", ["Launch"], "Water", 1.0, 2, accent=YEL))
E(["inflatable tube man", "wacky waving inflatable arm flailing tube man", "tube man", "air dancer"],
  "Inflatable tube man",
  R("Inflatable Tube Man", "Pillar", ["Repel", "Spin"], "Wind", 5.0, 5, accent=RED, temp="Chaotic", inten=1.0))
E(["bubble wrap"], "Bubble wrap",
  R("Bubble Wrap", "Wall", ["Barrier", "Bounce"], "Light", 3.0, 2))
E(["toaster", "toaster gun"], "Toaster",
  R("Toaster", "Cannon", ["Shoots"], "Fire", 0.6, 3, payload=R("toast", "Disc", ["Fast"], "Fire", 0.3, 1)))
E(["wrecking ball", "demolition ball"], "Wrecking ball",
  R("Wrecking Ball", "Whip", ["Blunt", "Heavy"], "Earth", 6.0, 3000))

# =============================================================== MYTHOLOGY & FOLKLORE
E(["kraken", "the kraken", "release the kraken", "giant squid monster"], "Norse / seafaring folklore",
  R("Kraken", "Kaiju", ["Constrict", "Grab"], "Water", 10.0, 80000, accent=(0.6, 0.3, 0.9), temp="Aggressive", inten=0.8),
  sh.kraken(), "grabs with two arms, constricts with the rest")
E(["hydra", "lernaean hydra", "many headed hydra"], "Greek mythology",
  R("Hydra", "Kaiju", ["Breath", "ArticulatedTail", "Swarm"], "Poison", 10.0, 50000, count=2, temp="Aggressive", inten=0.8),
  note="cut one down and there are two")
E(["cerberus", "kerberos", "three headed dog", "hellhound", "hell hound", "fluffy three headed dog"], "Greek mythology",
  R("Cerberus", "Quadruped", ["Breath", "Charge", "ArticulatedTail"], "Fire", 2.6, 400, accent=RED,
    temp="Aggressive", inten=0.9), sh.cerberus(), "three heads of fire and a serpent tail")
E(["fenrir", "fenris", "fenris wolf", "fenrisulfr"], "Norse mythology",
  R("Fenrir", "Quadruped", ["Charge", "Grab", "Heavy"], "Shadow", 4.0, 2000, temp="Aggressive", inten=1.0))
E(["gleipnir", "fenrir's chain"], "Norse mythology",
  R("Gleipnir", "Whip", ["Link", "Constrict"], "Psychic", 4.0, 1, accent=WHT), note="the ribbon that bound a god-wolf")
E(["jormungandr", "jörmungandr", "midgard serpent", "world serpent", "midgardsormr"], "Norse mythology",
  R("Jörmungandr", "Kaiju", ["Constrict", "Breath", "ArticulatedTail"], "Poison", 18.0, 200000, temp="Calm"))
E(["norse mjolnir", "mjöllnir", "mythic mjolnir", "thor's hammer (myth)"], "Norse mythology (Prose Edda)",
  R("Mjölnir (Norse)", "Hammer", ["Blunt", "Returning"], "Electric", 0.7, 30),
  note="the Eddic original: short-handled, always returns")
E(["gungnir", "odin's spear"], "Norse mythology",
  R("Gungnir", "Spear", ["Pierce", "Homing", "Returning"], "Electric", 2.2, 5, accent=GLD), note="never misses its mark")
E(["sleipnir", "eight legged horse", "odin's horse"], "Norse mythology",
  R("Sleipnir", "Quadruped", ["Fast", "Piloted", "Charge"], "Wind", 2.2, 700, temp="Protective", inten=0.6))
E(["valkyrie", "valkyries"], "Norse mythology",
  R("Valkyrie", "Biped", ["Flies", "Pierce"], "Light", 1.9, 70, feat=["Armor"], temp="Protective", inten=0.7))
E(["quetzalcoatl", "feathered serpent", "kukulkan", "quetzalcóatl"], "Aztec / Mesoamerican mythology",
  R("Quetzalcoatl", "Serpent", ["Flies", "Breath"], "Wind", 4.0, 150, accent=GRN, temp="Calm"),
  sh.quetzalcoatl(), "a feathered serpent that breathes gales")
E(["anubis", "jackal god"], "Egyptian mythology",
  R("Anubis", "Biped", ["Pierce", "Grab"], "Shadow", 2.3, 90, accent=GLD, temp="Calm"))
E(["medusa", "gorgon", "medusa's head"], "Greek mythology",
  R("Medusa", "Biped", ["Beam", "ArticulatedTail"], "Earth", 2.0, 70, temp="Sneaky", inten=0.8),
  note="a stony gaze beam (petrify = Earth's slow)")
E(["minotaur", "the minotaur", "bull man"], "Greek mythology",
  R("Minotaur", "Biped", ["Charge", "Heavy", "Blunt"], "Earth", 3.0, 400, accent=GLD, temp="Aggressive", inten=1.0),
  sh.minotaur(), "charges like a bull, swings a labrys")
E(["phoenix", "fire bird", "firebird", "fenghuang"], "Greek / Egyptian / Chinese mythology",
  R("Phoenix", "Bird", ["Flies", "Explodes", "Homing"], "Fire", 2.0, 15, accent=ORG, temp="Aggressive", inten=0.7),
  note="dives in and bursts into flame")
E(["thunderbird", "thunder bird"], "Indigenous North American mythology",
  R("Thunderbird", "Bird", ["Flies", "Beam", "Grab"], "Electric", 4.0, 300, temp="Protective", inten=0.6))
E(["kitsune", "nine tailed fox", "nine-tailed fox", "kyubi", "kumiho", "huli jing"], "Japanese / East Asian folklore",
  R("Kitsune", "Quadruped", ["Shoots", "Fast"], "Fire", 1.4, 20, accent=ORG,
    payload=R("foxfire", "Ball", ["Homing"], "Fire", 0.4, 1), temp="Sneaky", inten=0.8),
  sh.kitsune(), "nine tails and homing foxfire")
E(["oni", "japanese ogre", "oni demon"], "Japanese folklore",
  R("Oni", "Biped", ["Blunt", "Stomp", "Heavy"], "Fire", 2.8, 300, accent=RED, temp="Aggressive", inten=0.8),
  note="swings an iron kanabo club")
E(["gashadokuro", "giant skeleton", "starving skeleton"], "Japanese folklore",
  R("Gashadokuro", "Kaiju", ["Grab", "Stomp"], "Shadow", 14.0, 20000, temp="Sneaky", inten=0.7))
E(["jiangshi", "hopping vampire", "hopping zombie", "chinese vampire"], "Chinese folklore",
  R("Jiangshi", "Biped", ["Bounce", "Grab"], "Shadow", 1.8, 60, temp="Aggressive", inten=0.6), note="hops after you")
E(["kasa-obake", "umbrella yokai", "kasa obake", "karakasa"], "Japanese folklore (tsukumogami)",
  R("Kasa-obake", "Biped", ["Bounce", "SlowFall"], "Wind", 1.4, 3, temp="Playful", inten=1.0))
E(["sun wukong", "monkey king", "wukong"], "Journey to the West",
  R("Sun Wukong", "Biped", ["Swarm", "Fast", "Blunt"], "Wind", 1.6, 50, count=5, accent=GLD, temp="Playful", inten=0.9),
  note="plucks hairs into clones")
E(["ruyi jingu bang", "monkey king staff", "ruyi bang", "golden cudgel"], "Journey to the West",
  R("Ruyi Jingu Bang", "Staff", ["Blunt", "Spin", "Grow"], "Wind", 2.0, 80, accent=GLD),
  note="spins and makes what it hits huge")
E(["somersault cloud", "nimbus cloud", "jindou yun", "flying nimbus"], "Journey to the West",
  R("Somersault Cloud", "Platform", ["Flies", "Piloted", "Fast"], "Wind", 2.0, 5, accent=GLD))
E(["baba yaga's hut", "baba yaga hut", "hut on chicken legs", "chicken leg house", "izbushka"], "Slavic folklore",
  R("Baba Yaga's Hut", "Biped", ["Stomp", "Heavy", "Walks"], "Shadow", 4.0, 3000, temp="Chaotic", inten=0.8),
  sh.baba_yaga_hut(), "a house on chicken legs stomping after you")
E(["baba yaga", "baba jaga"], "Slavic folklore",
  R("Baba Yaga", "Vehicle", ["Flies", "Piloted", "Blunt"], "Shadow", 1.6, 60), note="flies in her mortar and pestle")
E(["golem", "clay golem", "stone golem"], "Jewish folklore",
  R("Golem", "Biped", ["Heavy", "Stomp", "Guard"], "Earth", 3.0, 2000, temp="Protective", inten=0.9))
E(["sphinx", "great sphinx", "riddle sphinx"], "Egyptian / Greek mythology",
  R("Sphinx", "Quadruped", ["Beam", "Guard"], "Psychic", 3.0, 5000, accent=GLD, temp="Lazy", inten=0.9),
  sh.sphinx(), "lies in the way and stares a riddle-beam at you")
E(["excalibur", "sword in the stone", "caliburn", "king arthur's sword"], "Arthurian legend",
  R("Excalibur", "Sword", ["Blade", "Beam"], "Light", 1.3, 3, accent=GLD), sh.excalibur(),
  "a holy blade that fires a beam of light")
E(["aegis", "zeus's shield", "athena's shield"], "Greek mythology",
  R("Aegis", "Shield", ["Guard", "Repel"], "Electric", 1.3, 10, accent=GLD))
E(["trojan horse", "wooden horse", "horse of troy"], "Greek mythology (Iliad/Aeneid)",
  R("Trojan Horse", "Quadruped", ["Shoots", "Heavy"], "Light", 5.0, 6000, accent=BRN, feat=["Wheels"],
    payload=R("greek hoplite", "Biped", ["Pierce", "Guard"], "Light", 1.8, 80, temp="Aggressive", inten=0.8),
    temp="Sneaky", inten=1.0), sh.trojan_horse(), "a gift! it rolls up and releases soldiers")
E(["pandora's box", "pandoras box", "pandora box"], "Greek mythology",
  R("Pandora's Box", "Bomb", ["Explodes", "Shoots"], "Shadow", 0.8, 5, accent=GLD,
    payload=R("evils", "Bird", ["Swarm", "Homing", "Flies"], "Shadow", 0.4, 1, count=6, temp="Chaotic", inten=1.0)),
  note="opens: a swarm of evils (and hope, somewhere)")
E(["pegasus", "winged horse"], "Greek mythology",
  R("Pegasus", "Quadruped", ["Flies", "Piloted", "Fast"], "Wind", 2.2, 500, accent=WHT, temp="Protective", inten=0.6))
E(["griffin", "gryphon", "griffon"], "Greek / heraldic",
  R("Griffin", "Quadruped", ["Flies", "Grab"], "Wind", 2.4, 400, accent=GLD, temp="Aggressive", inten=0.7))
E(["chimera", "chimaera"], "Greek mythology",
  R("Chimera", "Quadruped", ["Breath", "ArticulatedTail", "Charge"], "Fire", 2.4, 500, temp="Aggressive", inten=0.8))
E(["manticore"], "Persian mythology",
  R("Manticore", "Quadruped", ["ArticulatedTail", "Shoots", "Charge"], "Poison", 2.4, 400, temp="Aggressive", inten=0.8,
    payload=R("tail spike", "Spear", ["Pierce"], "Poison", 0.6, 1)))
E(["polyphemus", "greek cyclops", "cyclops giant"], "Greek mythology (Odyssey)",
  R("Polyphemus", "Kaiju", ["Stomp", "Shoots"], "Earth", 10.0, 20000,
    payload=R("boulder", "Boulder", ["Heavy"], "Earth", 2.0, 2000), temp="Aggressive", inten=0.7), note="throws boulders")
E(["talaria", "hermes sandals", "winged sandals", "hermes' sandals"], "Greek mythology",
  R("Talaria", "Wings", ["Glide", "Fast"], "Wind", 0.6, 1, accent=GLD))
E(["icarus wings", "wings of icarus", "icarus"], "Greek mythology",
  R("Wings of Icarus", "Wings", ["Glide"], "Fire", 3.0, 5), note="fly too close to the sun (fire element)")
E(["ouroboros"], "Egyptian / Greek symbol",
  R("Ouroboros", "Disc", ["Spin", "Returning", "Constrict"], "Poison", 1.5, 10), note="a snake-ring that eats its own tail")
E(["loch ness monster", "nessie", "loch ness"], "Scottish folklore",
  R("Nessie", "Serpent", ["Constrict", "Grab"], "Water", 4.0, 1500, temp="Sneaky", inten=0.8),
  sh.nessie(), "surfaces, grabs, and wraps")
E(["bigfoot", "sasquatch", "yowie"], "North American folklore",
  R("Bigfoot", "Biped", ["Stomp", "Fast"], "Earth", 2.6, 250, temp="Sneaky", inten=1.0), note="always slightly out of focus")
E(["yeti", "abominable snowman", "migoi"], "Himalayan folklore",
  R("Yeti", "Biped", ["Stomp", "Breath", "Grab"], "Ice", 2.8, 300, temp="Aggressive", inten=0.7))
E(["mothman", "moth man"], "West Virginia folklore",
  R("Mothman", "Insect", ["Flies", "Beam"], "Shadow", 2.0, 60, temp="Sneaky", inten=1.0), note="glowing red eyes, a gaze beam")
E(["chupacabra", "chupacabras"], "Latin American folklore",
  R("Chupacabra", "Quadruped", ["Charge", "Grab", "Fast"], "Poison", 1.2, 40, temp="Sneaky", inten=0.9))
E(["wendigo", "windigo"], "Algonquian folklore",
  R("Wendigo", "Biped", ["Charge", "Grab", "Fast"], "Ice", 3.0, 120, temp="Sneaky", inten=1.0))
E(["krampus"], "Alpine folklore",
  R("Krampus", "Biped", ["Grab", "Pull"], "Shadow", 2.4, 120, temp="Chaotic", inten=0.9, feat=["Chains", "Bell"]),
  note="chains you and drags you off")
E(["dullahan", "headless horseman"], "Irish folklore / Sleepy Hollow",
  R("Dullahan", "Quadruped", ["Charge", "Piloted", "Fast"], "Shadow", 2.4, 700, temp="Aggressive", inten=0.8))
E(["banshee", "bean sídhe"], "Irish folklore",
  R("Banshee", "Blob", ["Flies", "Repel"], "Shadow", 1.8, 5, temp="Fearful", inten=0.8), note="a wail that blows you back")
E(["tanuki", "bake-danuki"], "Japanese folklore",
  R("Tanuki", "Quadruped", ["Grow", "Bounce"], "Light", 1.0, 15, temp="Playful", inten=0.9), note="shapeshifter: makes things huge")
E(["kappa", "kawataro"], "Japanese folklore",
  R("Kappa", "Biped", ["Grab", "Pull"], "Water", 1.2, 30, temp="Sneaky", inten=0.7))
E(["tengu", "karasu tengu"], "Japanese folklore",
  R("Tengu", "Bird", ["Flies", "Repel", "Blade"], "Wind", 1.8, 50, temp="Aggressive", inten=0.6), note="fan gusts and swordplay")
E(["cthulhu", "great cthulhu", "cthulu"], "H. P. Lovecraft (1928)",
  R("Cthulhu", "Kaiju", ["Grab", "Constrict", "Flies"], "Psychic", 16.0, 100000, temp="Chaotic", inten=0.9))
E(["tiamat", "five headed dragon"], "Babylonian myth / D&D",
  R("Tiamat", "Kaiju", ["Breath", "Flies", "ArticulatedTail"], "Poison", 14.0, 60000, temp="Aggressive", inten=0.9))
E(["leviathan"], "Hebrew Bible",
  R("Leviathan", "Kaiju", ["Breath", "Constrict", "ArticulatedTail"], "Water", 18.0, 150000))
E(["behemoth"], "Hebrew Bible",
  R("Behemoth", "Kaiju", ["Stomp", "Charge", "Heavy"], "Earth", 14.0, 150000, temp="Lazy", inten=0.5))

# =============================================================== D&D
DND = "Dungeons & Dragons"
E(["beholder", "beholders", "eye tyrant", "floating eye monster"], DND,
  R("Beholder", "Blob", ["Flies", "Beam", "Grab"], "Psychic", 2.4, 200, temp="Aggressive", inten=0.8),
  sh.beholder(), "a floating eye tyrant: eye-beams and a bite")
E(["mimic", "chest mimic", "treasure chest monster", "mimic chest"], DND,
  R("Mimic", "Blob", ["Grab", "Trap", "Sticky"], "Poison", 1.4, 100, accent=GLD, temp="Sneaky", inten=1.0),
  sh.mimic(), "it was never a chest: sticky tongue, snapping lid")
E(["gelatinous cube", "gel cube", "jelly cube"], DND,
  R("Gelatinous Cube", "Blob", ["Trap", "Sticky"], "Poison", 1.6, 300, accent=GRN, temp="Lazy", inten=0.9),
  sh.gelatinous_cube(), "slides over you and engulfs")
E(["owlbear", "owl bear"], DND,
  R("Owlbear", "Quadruped", ["Grab", "Charge"], "Light", 2.4, 600, temp="Aggressive", inten=0.9))
E(["tarrasque"], DND,
  R("Tarrasque", "Kaiju", ["Stomp", "Charge", "ArticulatedTail"], "Earth", 18.0, 130000, temp="Aggressive", inten=1.0))
E(["mind flayer", "illithid", "mindflayer", "d&d mind flayer"], DND,
  R("Mind Flayer", "Biped", ["Breath", "Grab"], "Psychic", 1.9, 70, temp="Sneaky", inten=0.8), note="psionic mind-blast cone + tentacles")
E(["displacer beast"], DND,
  R("Displacer Beast", "Quadruped", ["ArticulatedTail", "Portal", "Fast"], "Shadow", 2.0, 250, temp="Sneaky", inten=0.9))
E(["bulette", "land shark"], DND,
  R("Bulette", "Fish", ["Walks", "Charge", "Heavy"], "Earth", 2.6, 900, temp="Aggressive", inten=0.8))
E(["rust monster"], DND,
  R("Rust Monster", "Insect", ["Grab", "Walks"], "Poison", 1.4, 40, feat=["Antenna"], temp="Aggressive", inten=0.7))
E(["bag of holding", "bag of holding (d&d)"], DND,
  R("Bag of Holding", "Cage", ["Attract", "Trap"], "Shadow", 1.0, 5), note="sucks things in")
E(["portable hole", "d&d portable hole"], DND,
  R("Portable Hole", "Platform", ["Portal"], "Shadow", 1.8, 1))
E(["immovable rod"], DND,
  R("Immovable Rod", "Pillar", ["Barrier", "Heavy"], "Light", 1.0, 99999, accent=GRY), sh.immovable_rod(),
  "press the button: it does not move. ever. a tether anchor in mid-air")
E(["magic missile"], DND,
  R("Magic Missile", "Staff", ["Shoots", "Homing"], "Light", 1.2, 1,
    payload=R("missile", "Rocket", ["Homing", "Fast"], "Light", 0.3, 1, count=3)), note="three darts that never miss")
E(["eldritch blast", "agonizing blast"], DND,
  R("Eldritch Blast", "Staff", ["Beam", "Repel"], "Shadow", 1.2, 1), note="with Repelling Blast, naturally")
E(["sphere of annihilation"], DND,
  R("Sphere of Annihilation", "Ball", ["Attract", "Heavy"], "Shadow", 1.2, 10))
E(["vorpal sword", "vorpal blade"], "Jabberwocky (Lewis Carroll) / D&D",
  R("Vorpal Sword", "Sword", ["Blade", "Fast"], "Light", 1.1, 2, accent=WHT), note="snicker-snack")
E(["deck of many things"], DND,
  R("Deck of Many Things", "Disc", ["Swarm", "Homing"], "Psychic", 0.4, 1, count=5, temp="Chaotic", inten=1.0))
E(["i attack the gazebo", "gazebo", "the gazebo"], "D&D 'Eric and the Gazebo' (1970s)",
  R("The Gazebo", "Dome", ["Barrier", "Heavy"], "Light", 4.0, 5000), note="it is, and remains, a gazebo")
E(["flumph"], DND,
  R("Flumph", "Blob", ["Flies", "Repel"], "Poison", 1.0, 10, temp="Fearful", inten=0.6))
E(["lich", "lich king (d&d)"], DND,
  R("Lich", "Biped", ["Beam", "Flies"], "Ice", 1.9, 40, temp="Calm"))
E(["kobold"], DND,
  R("Kobold", "Biped", ["Swarm", "Shoots"], "Light", 0.8, 15, count=5, temp="Sneaky", inten=0.7))

import glob, re as _re
OTHER = set()
for _p in glob.glob(str(ROOT / "assets/icons.ron")) + glob.glob(str(ROOT / "assets/icons/*.ron")):
    if _p.replace("\\", "/").endswith("internet.ron"):
        continue
    _s = open(_p, encoding="utf-8").read()
    for _m in _re.finditer(r"names:\s*\[(.*?)\]", _s, _re.S):
        OTHER.update(_re.findall(r'"([^"]*)"', _m.group(1)))
# Meme-y names this pack owns even if another pack also lists them (lead decision).
KEEP = {"amogus", "among us", "amongus", "among us crewmate", "crewmate", "impostor", "imposter", "sus", "amogus sus",
        "banana peel"}
OTHER -= KEEP
DROP_IDS = {"beyblade", "acme anvil", "cardboard box", "boxing glove", "excalibur", "cthulhu",
            "duolingo owl", "hydra"}
_kept = []
for (names, source, r, shape, note) in ENTRIES:
    if names[0] in DROP_IDS:
        continue
    nn = [n for n in names if n not in OTHER]
    if not nn:
        print("  dropped (all names taken):", names[0]); continue
    if nn != names:
        print("  renamed", names[0], "->", nn[0], "removed", [n for n in names if n in OTHER])
    _kept.append((nn, source, r, shape, note))
ENTRIES[:] = _kept
UP = {"labubu": 2.0, "chimpanzini bananini": 1.8, "cappuccino assassino": 1.5, "moo deng": 1.8, "pesto": 1.6,
      "chill guy": 1.4, "pedro": 1.8, "jimothy": 1.5, "capybara": 1.5, "doge": 1.8, "nyan cat": 1.6, "amogus": 1.6,
      "rubber duck": 1.5, "lego": 2.0, "duolingo owl": 1.5, "clippy": 1.4, "ferris": 1.6, "chrome dino": 1.5,
      "roomba": 1.5, "nokia 3310": 1.4, "hungry hungry hippos": 1.6, "skibidi toilet": 1.5, "tralalero tralala": 1.4,
      "tung tung tung sahur": 1.3, "kitsune": 1.6, "loch ness monster": 1.5, "quetzalcoatl": 1.6, "mimic": 1.4,
      "traffic cone": 1.3, "banana peel": 1.5, "shopping cart": 1.3, "bowling ball": 1.5, "curling stone": 1.3,
      "gigachad": 1.2, "shrek": 1.2, "gorilla vs 100 men": 1.2, "baba yaga's hut": 1.3, "immovable rod": 1.5,
      "beholder": 1.2}
for i, (names, source, r, shape, note) in enumerate(ENTRIES):
    k = UP.get(names[0])
    if shape is not None and k:
        shape = scaled(shape, k)
        r["scale"] = round(r["scale"] * k, 2)
        ENTRIES[i] = (names, source, r, shape, note)
    if shape is not None:
        check(shape, names[0])
out, seen = emit()
open(sys.argv[1] if len(sys.argv) > 1 else ROOT / "assets/icons/internet.ron", "w", encoding="utf-8").write(out)
print(len(ENTRIES), "entries,", sum(1 for e in ENTRIES if e[3] is not None), "shapes")
