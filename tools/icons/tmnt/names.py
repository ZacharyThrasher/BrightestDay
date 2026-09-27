"""Generate assets/icons/tmnt_names.txt: TMNT characters/vehicles/items beyond the hand-authored icons.

    python tools/icons/tmnt/names.py

Format (merged by tools/lexicon_build/build.py::load_names_txt):
    name | alias; alias | QID or blank | hint: Form + Mechanic, Mechanic; Element; Temperament [notes]
Names already used by any icon pack (*.ron) or another *_names.txt are dropped (and reported), so this file
never fights another pack. Everyday-word names are qualified ('tattoo tmnt'). Names/facts only.
"""
import glob
import os
import re

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.abspath(os.path.join(HERE, "..", "..", ".."))

# (name, [aliases], hint)  -- QIDs left blank (unresolved) rather than guessed.
N = [
    # ---------------------------------------------------------------- 1987 cartoon: humans & allies
    ("irma langinstein", ["irma tmnt"], "Biped + Fast; Fearful [1987; April's friend]"),
    ("burne thompson", ["burne thompson channel 6"], "Biped + Shoots; Fearful [1987; Channel 6 boss]"),
    ("vernon fenwick", ["vernon tmnt"], "Biped + Fast; Fearful [1987/2012; cameraman]"),
    ("zach the fifth turtle", ["zach tmnt"], "Biped + Fast, Bounce; Playful [1987]"),
    ("chrome dome", ["chrome dome tmnt"], "Biped + Blade, Shoots, Armor; Electric; Aggressive [1987/2012 robot]"),
    ("tempestra", ["tempestra tmnt"], "Biped + Shoots, Beam; Electric; Chaotic [1987; video-game villainess]"),
    ("pizza face", ["pizza face tmnt"], "Blob + Shoots, Sticky; Fire; Chaotic [1987/2012 mutant pizza]"),
    ("pizza monster", ["pizza monsters"], "Blob + Swarm, Sticky, Grab; Chaotic [2012 / Shredder's Revenge]"),
    ("hi-tech", ["hi tech tmnt", "hitech"], "Insect + Shoots; Electric; Sneaky [1987; Dregg's aide]"),
    ("technogang", ["techno gang", "dregg's technogang"], "Insect + Swarm, Shoots; Electric; Aggressive [1987]"),
    ("mung the manipulator", [], "Biped + Psychic, Shoots; Sneaky [1987]"),
    ("carter tmnt", ["carter the mutant"], "Biped + Blunt, Fast; Protective [1987 season 10]"),
    ("captain zorax", ["zorax"], "Biped + Charge, Shoots, Heavy; Aggressive [1987 Triceraton leader]"),
    ("kala neutrino", ["kala the neutrino"],"Biped + Flies, Fast; Playful [1987 Neutrino]"),
    ("dask neutrino", ["dask"], "Biped + Flies, Fast; Playful [1987 Neutrino]"),
    ("trib neutrino", ["trib"], "Biped + Flies, Fast; Playful [1987 Neutrino]"),
    ("neutrinos", ["the neutrinos", "neutrino tmnt"], "Biped + Swarm, Flies, Fast; Playful [1987]"),
    ("neutrino flying car", ["neutrinos car", "neutrino car"], "Vehicle + Flies, Fast, Piloted [1987]"),
    ("renet", ["renet tilley", "renet time apprentice"], "Biped + Portal, Fast; Psychic [Mirage/1987/2012]"),
    ("lord simultaneous", [], "Biped + Portal, Beam; Psychic [Mirage/1987]"),
    ("leatherhead 1987", ["cajun leatherhead"], "Biped + Grab, ArticulatedTail; Water; Aggressive [1987]"),
    ("shogun shredder", ["tengu shredder", "ninja tribunal shredder"], "Biped + Blade, Breath; Fire; Aggressive [2003 demon]"),
    ("cyber shredder", ["cybershredder"], "Biped + Blade, Fast; Electric; Aggressive [2003 Fast Forward / Tournament Fighters]"),
    ("ch'rell", ["chrell", "utrom shredder"], "Blob + Grab, Psychic; Sneaky [2003 Utrom in the exo-suit]"),
    ("shredder's exo-suit", ["ch'rell's exo-suit", "utrom exo suit", "shredder exosuit"], "Biped + Piloted, Blade, Heavy [2003]"),
    ("krang's android", ["krang android 1987"], "Biped + Piloted, Blunt, Heavy [1987 alt spelling]"),
    ("super krang", ["giant krang"], "Kaiju + Stomp, Grab, Heavy; Psychic [Turtles in Time final boss]"),
    ("general krang", ["krang idw"], "Biped + Piloted, Shoots; Psychic [IDW]"),
    ("kraang subprime", ["kraang sub-prime"], "Biped + Piloted, Shoots, Grab; Psychic [2012]"),
    ("kraang walker", ["kraang stealth walker"], "Arachnid + Shoots, Piloted, Heavy [2012]"),
    ("mortu", ["utrom mortu"], "Blob + Guard; Psychic; Protective [2003 Utrom leader]"),
    ("fugitoid", ["professor honeycutt", "honeycutt robot"], "Biped + Shoots, Portal; Electric; Fearful [Mirage/2003/2012]"),
    ("cudley the cowlick", ["cudley", "cudley cowlick"], "Blob + Flies, Portal; Calm [Mirage; giant cow head]"),
    ("stainless steve steel", [], "Biped + Blunt, Charge; Aggressive [2003]"),
    ("nobody tmnt", ["the nobody tmnt"], "Biped + Fast, Blade; Sneaky [Mirage/2003 vigilante]"),
    ("silver sentry", [], "Biped + Flies, Blunt, Guard; Protective [2003 Justice Force]"),
    ("justice force", ["the justice force tmnt"], "Biped + Swarm, Flies, Blunt; Protective [2003]"),
    ("garbageman tmnt", ["garbage man tmnt"], "Biped + Grab, Sticky; Poison; Chaotic [2003]"),
    ("nano tmnt", ["nano nanobots"], "Blob + Swarm, Grab; Electric; Chaotic [2003 living nanobots]"),
    ("dark turtles", ["the dark turtles"], "Biped + Swarm, Blade, Fast; Shadow; Aggressive [2003]"),
    ("sh'okanabo", ["shokanabo"], "Blob + Grab, Swarm; Poison; Chaotic [2003]"),
    ("ultimate drako", ["drako tmnt", "drako"], "Biped + Portal, Flies, Blade; Shadow; Aggressive [2003]"),
    ("ultimate ninja", ["the ultimate ninja"], "Biped + Blade, Fast; Shadow; Sneaky [2003]"),
    ("ninja tribunal", ["the ninja tribunal"], "Biped + Swarm, Blade, Psychic; Protective [2003]"),
    ("dr chaplin", ["doctor chaplin", "chaplin tmnt"], "Biped + Shoots; Fearful [2003]"),
    ("darius dun", [], "Biped + Shoots, Fast; Sneaky [2003 Fast Forward]"),
    ("viral tmnt", ["viral the virus"], "Blob + Swarm, Sticky; Electric; Chaotic [2003 Fast Forward]"),
    ("serling tmnt", ["serling robot"], "Biped + Guard; Electric; Protective [2003 Fast Forward]"),
    ("cody jones", [], "Biped + Shoots; Fearful [2003 Fast Forward]"),
    ("klunk", ["klunk the cat", "mikey's cat"], "Quadruped + Fast, Bounce; Playful [2003]"),
    ("commander mozar", ["mozar"], "Biped + Charge, Shoots; Aggressive [2003 Triceraton]"),
    ("the ancient one tmnt", ["ancient one tmnt"], "Biped + Blunt, Fast; Protective [2003 Splinter's master]"),
    ("hamato yoshi", ["yoshi hamato"], "Biped + Blade, Fast; Protective [Splinter's master/human self]"),
    ("the collector tmnt", [], "Biped + Trap, Grab; Sneaky [2012]"),
    # ---------------------------------------------------------------- 2012 series
    ("tiger claw", ["takeshi tiger claw"], "Biped + Blade, Fast, Shoots; Aggressive [2012 tiger mutant]"),
    ("dogpound", ["dog pound tmnt", "chris bradford"], "Biped + Blunt, Heavy, Charge; Aggressive [2012]"),
    ("fishface", ["fish face tmnt", "xever"], "Fish + Walks, Pierce, Fast; Water; Aggressive [2012 fish w/ robot legs]"),
    ("spider bytez", ["spider bites tmnt", "vic spider bytez"], "Arachnid + Shoots, Sticky; Poison; Chaotic [2012]"),
    ("squirrelanoids", ["squirrelanoid"], "Quadruped + Swarm, Grab, Fast; Aggressive [2012]"),
    ("snakeweed", ["snake weed tmnt"], "Serpent + Constrict, Grab; Poison; Aggressive [2012 plant mutant]"),
    ("pigeon pete", ["pete the pigeon"], "Bird + Flies, Fast; Playful [2012/IDW]"),
    ("ice cream kitty", [], "Quadruped + Sticky, Bounce; Ice; Playful [2012]"),
    ("spy roach", ["spyroach"], "Insect + Flies, Shoots; Electric; Sneaky [2012 mutant cockroach]"),
    ("newtralizer", ["the newtralizer"], "Biped + Shoots, Charge; Poison; Aggressive [2012 salamander alien]"),
    ("sal commander", ["salamandrian commander"], "Biped + Shoots, Fast; Water; Protective [2012]"),
    ("armaggon", ["armaggon shark", "armageddon shark tmnt"], "Fish + Walks, Grab, Charge; Water; Aggressive [Archie/Tournament Fighters/2012]"),
    ("parasitica", ["parasitica wasp"], "Insect + Flies, Pierce, Swarm; Poison; Aggressive [2012]"),
    ("pulverizer", ["the pulverizer", "timothy pulverizer"], "Blob + Walks, Sticky; Poison; Playful [2012]"),
    ("fungus humungous", [], "Blob + Swarm, Sticky; Poison; Sneaky [2012]"),
    ("big mama tmnt", ["big mama spider"], "Arachnid + Grab, Sticky, Pierce; Poison; Sneaky [2012/Rise]"),
    ("shinigami tmnt", ["shinigami karai friend"], "Biped + Blade, Psychic; Sneaky [2012]"),
    ("kavaxas", [], "Serpent + Flies, Breath; Shadow; Aggressive [2012 demon]"),
    ("alopex", ["alopex fox"], "Biped + Blade, Fast; Ice; Sneaky [IDW/2012 arctic fox]"),
    ("bullfrog tmnt", ["bullfrog punk frog"], "Biped + Bounce, Charge; Water; Aggressive [2012 Punk Frogs]"),
    ("napoleon bonafrog", ["napoleon frog"], "Biped + Bounce, Fast; Water; Playful [1987/2012/Mutant Mayhem]"),
    ("genghis frog", ["genghis the frog"], "Biped + Bounce, Blade; Water; Aggressive [1987/Mutant Mayhem]"),
    ("attila the frog", [], "Biped + Bounce, Blunt; Water; Aggressive [1987 Punk Frogs]"),
    ("rasputin the mad frog", ["rasputin frog"], "Biped + Bounce, Grab; Water; Chaotic [1987 Punk Frogs]"),
    ("punk frogs", ["the punk frogs"], "Biped + Swarm, Bounce; Water; Playful [1987/2012]"),
    ("kirby o'neil", ["kirby oneil"], "Biped + Flies; Fearful [2012 April's dad]"),
    ("leatherhead 2012", [], "Biped + Grab, ArticulatedTail, Charge; Water; Protective [2012]"),
    ("chris bradford", [], "Biped + Blade, Fast; Aggressive [2012 human form of Dogpound/Rahzar]"),
    ("patrol buggy", ["the patrol buggy"], "Vehicle + Wheels, Fast, Piloted [2012]"),
    ("stealth bike", ["raph's stealth bike"], "Vehicle + Wheels, Fast, Piloted; Sneaky [2012]"),
    ("turtle mech", ["metalhead mech"], "Biped + Piloted, Blunt, Heavy [2012]"),
    ("t-pod", ["tpod"], "Vehicle + Wheels, Fast, Piloted [2012]"),
    ("shell sub", ["turtle sub", "shellsub"], "Vehicle + Piloted, Fast; Water [2003/2012 submarine]"),
    ("turtle glider", ["shell glider"], "Wings + Glide, SlowFall [2003/2012]"),
    ("battle shell", ["the battle shell"], "Vehicle + Wheels, Piloted, Shoots, Armor [2003 truck]"),
    ("shell cycle", ["turtle bike"], "Vehicle + Wheels, Fast, Piloted [2003/2012]"),
    ("turtle cycle", ["the turtle cycle"], "Vehicle + Wheels, Fast, Piloted [1987 toy]"),
    ("cheapskate", ["cheapskate tmnt"], "Vehicle + Wheels, Fast, Launch [1987 toy skateboard]"),
    ("sewer surfer", ["sewer surfer tmnt"], "Ramp + Launch, Fast; Water [1987 toy]"),
    ("footski", ["foot ski"], "Vehicle + Fast, Piloted; Water [1987 toy]"),
    ("psycho cycle", ["shredder's psycho cycle"], "Vehicle + Wheels, Charge, Piloted, Spikes [1989 toy]"),
    ("turtle trooper", ["turtle trooper parachute"], "Parachute + SlowFall, Glide [1987 toy]"),
    ("foot mechs", ["foot mech"], "Biped + Piloted, Shoots, Heavy; Aggressive [2003]"),
    ("foot elite", ["elite foot ninja", "foot elite guard"], "Biped + Swarm, Pierce, Fast; Sneaky [2003/2012]"),
    ("foot recruits", ["foot recruit"], "Biped + Swarm, Blunt; Playful [Rise]"),
    ("foot lieutenant", [], "Biped + Blade, Fast; Aggressive [Rise]"),
    ("kraang army", [], "Biped + Swarm, Shoots; Psychic [2012]"),
    # ---------------------------------------------------------------- Rise of the TMNT
    ("baron draxum", ["draxum"], "Biped + Blade, Grab; Poison; Aggressive [Rise]"),
    ("meat sweats", ["meatsweats"], "Biped + Blunt, Heavy; Fire; Aggressive [Rise pig chef]"),
    ("warren stone", [], "Serpent + Constrict, Walks; Earth; Fearful [Rise worm]"),
    ("hypno-potamus", ["hypnopotamus"], "Biped + Heavy, Psychic; Sneaky [Rise hippo]"),
    ("repo mantis", ["repomantis"], "Insect + Blade, Grab; Chaotic [Rise]"),
    ("todd scaramucci", ["todd capybara"], "Quadruped + Fast; Playful [Rise capybara]"),
    ("albearto", ["albearto the bear"], "Biped + Blunt, Heavy; Playful [Rise]"),
    ("señor hueso", ["senor hueso"], "Biped + Blade, Fast; Shadow; Sneaky [Rise skeleton cat]"),
    ("lou jitsu", ["splinter lou jitsu"], "Biped + Blunt, Fast, Charge; Playful [Rise Splinter's movie-star self]"),
    ("ghostbear", ["ghost bear tmnt"], "Biped + Grab, Heavy, Charge; Aggressive [Rise wrestler]"),
    ("shelldon", ["shelldon drone"], "Bird + Flies, Shoots; Electric; Playful [Rise Donnie's drone]"),
    ("mayhem tmnt", ["mayhem the pet"], "Quadruped + Portal, Fast; Playful [Rise teleporting dog]"),
    ("big mama rise", [], "Arachnid + Grab, Sticky; Sneaky [Rise]"),
    ("leonardo's odachi", ["leo's odachi", "odachi of leonardo"], "Sword + Blade, Portal [Rise ninpo portal sword]"),
    ("raph's tonfa", ["raphael's tonfa", "raph tonfa"], "Gauntlet + Blunt, Guard [Rise]"),
    ("mikey's kusarigama", ["michelangelo's kusarigama"], "Whip + Blade, Pull [Rise]"),
    ("donnie's tech bo", ["donatello's tech bo"], "Spear + Blunt, Thrust; Electric [Rise]"),
    ("turtle tank rise", ["rise turtle tank"], "Vehicle + Treads, Piloted, Shoots [Rise]"),
    ("puppy paw", ["puppy paw van"], "Vehicle + Wheels, Piloted, Shoots [Rise van]"),
    # ---------------------------------------------------------------- Mutant Mayhem (2023)
    ("superfly", ["superfly tmnt", "super fly tmnt"], "Kaiju + Flies, Grab, Sticky; Poison; Aggressive [Mutant Mayhem]"),
    ("scumbug", ["scum bug"], "Insect + Swarm, Sticky; Poison; Chaotic [1987 toy/Mutant Mayhem]"),
    ("ray fillet", ["rayfillet"], "Fish + Flies, Glide; Electric; Protective [1987 toy/Mutant Mayhem]"),
    ("cynthia utrom", [], "Biped + Shoots; Sneaky [Mutant Mayhem]"),
    ("wingnut mutant mayhem", ["mutant mayhem wingnut"], "Biped + Flies, Blade, Fast; Chaotic [Mutant Mayhem bat]"),
    ("leatherhead mutant mayhem", [], "Biped + Grab, ArticulatedTail; Water; Aggressive [Mutant Mayhem]"),
    ("mutant mayhem squad", ["superfly's gang"], "Biped + Swarm, Charge; Chaotic [Mutant Mayhem]"),
    # ---------------------------------------------------------------- IDW & Archie
    ("old hob", ["hob tmnt", "old hob the cat"], "Biped + Blade, Fast, Shoots; Sneaky [IDW alley cat]"),
    ("kitsune tmnt", ["kitsune fox spirit"], "Biped + Psychic, Portal; Sneaky [IDW]"),
    ("koya tmnt", ["koya hawk"],"Bird + Flies, Grab; Aggressive [IDW]"),
    ("bludgeon", ["bludgeon shark"], "Biped + Blunt, Heavy, Grab; Water; Aggressive [IDW hammerhead]"),
    ("jennika", ["jennika turtle", "sixth turtle"], "Biped + Blade, Fast; Aggressive [IDW turtle]"),
    ("lita tmnt", ["lita turtle"], "Biped + Fast, Bounce; Playful [IDW]"),
    ("mutanimals", ["mighty mutanimals", "the mutanimals"], "Biped + Swarm, Blunt, Fast; Protective [Archie/IDW]"),
    ("sally pride", [], "Biped + Shoots, Fast; Sneaky [IDW bounty hunter]"),
    ("ninjara", ["ninjara fox"], "Biped + Blade, Fast; Sneaky [Archie fox kunoichi]"),
    ("man ray", ["manray tmnt"], "Fish + Walks, Guard; Water; Protective [Archie/1987 toy]"),
    ("jagwar", ["jagwar tmnt"], "Biped + Blade, Fast; Aggressive [Archie jaguar]"),
    ("dreadmon", [], "Biped + Blade, Fast; Aggressive [Archie]"),
    ("wyrm tmnt", ["wyrm the worm"], "Serpent + Constrict, Grab; Earth; Chaotic [1987 toy/Tournament Fighters]"),
    ("maligna", ["queen maligna"], "Insect + Flies, Grab; Poison; Aggressive [Archie]"),
    ("merdude", ["mer dude"], "Fish + Walks, Pierce; Water; Protective [1987 toy]"),
    ("ace duck", ["ace duck tmnt"], "Bird + Flies, Shoots; Playful [1987 toy]"),
    ("antrax", ["antrax the ant"], "Insect + Swarm, Pierce; Poison; Aggressive [1987 toy]"),
    ("sergeant bananas", ["sgt bananas", "sgt. bananas"], "Biped + Blunt, Charge; Aggressive [1987 toy/Shredder's Revenge]"),
    ("monty moose", ["monty moose tmnt"], "Quadruped + Charge, Heavy; Protective [1987 toy]"),
    ("walkabout tmnt", ["walkabout kangaroo"], "Biped + Bounce, Charge; Playful [1987 toy]"),
    ("hothead dragon tmnt", ["hothead tmnt"], "Kaiju + Flies, Breath; Fire; Aggressive [1987 toy dragon]"),
    ("tattoo tmnt", ["tattoo the mutant"], "Biped + Blunt, Charge; Aggressive [1987 toy]"),
    ("halfcourt tmnt", ["halfcourt"], "Biped + Bounce, Fast; Playful [1987 toy basketball]"),
    ("headdrop tmnt", ["headdrop"], "Insect + Flies, Grab; Chaotic [1987 toy]"),
    ("doctor el", ["dr el"], "Biped + Shoots; Sneaky [1987 toy]"),
    # ---------------------------------------------------------------- games
    ("cement man", ["cementman"], "Biped + Blunt, Heavy, Stomp; Earth; Aggressive [Turtles in Time]"),
    ("tora tmnt", ["tora polar bear"], "Biped + Blunt, Heavy; Ice; Aggressive [Manhattan Project]"),
    ("aska", ["aska tmnt"], "Biped + Blade, Fast; Sneaky [Tournament Fighters]"),
    ("war tmnt", ["war tournament fighters"], "Biped + Grab, Heavy; Aggressive [Tournament Fighters]"),
    ("sisyphus tmnt", ["sisyphus beetle"], "Insect + Charge, Heavy, Armor; Aggressive [Tournament Fighters]"),
    ("triborg", ["tri-borg"], "Biped + Shoots, Heavy, Armor; Electric; Aggressive [Tournament Fighters]"),
    ("rat king 1987", ["rat king cartoon"], "Biped + Shoots, Swarm; Sneaky [1987]"),
    ("shredder's revenge", ["tmnt shredder's revenge"], "Biped + Swarm, Blade, Fast; Playful [2022 game: the turtles]"),
    ("turtles in time", ["tmnt turtles in time"], "Biped + Swarm, Portal, Fast; Playful [1991 game]"),
    ("the manhattan project tmnt", [], "Biped + Swarm, Fast; Playful [1991 game]"),
    ("roadkill rodney", ["roadkill rodney robot"], "Vehicle + Wheels, Shoots, Fast; Aggressive [1987 robots]"),
    ("roadkill rodneys", [], "Vehicle + Swarm, Wheels, Shoots; Aggressive [1987]"),
    ("stone warrior", ["stone warriors traag"], "Biped + Heavy, Blunt; Earth [1987]"),
    # ---------------------------------------------------------------- films
    ("tatsu", ["tatsu foot"], "Biped + Blunt, Fast; Aggressive [1990 film]"),
    ("danny pennington", [], "Biped + Fast; Fearful [1990 film]"),
    ("keno tmnt", ["keno the pizza boy"],"Biped + Blunt, Fast; Playful [1991 film]"),
    ("professor jordan perry", ["jordan perry"], "Biped + Shoots; Protective [1991 film; TGRI scientist]"),
    ("lord norinaga", ["norinaga"], "Biped + Blade, Shoots; Aggressive [1993 film]"),
    ("walker tmnt", ["walker the trader"], "Biped + Shoots; Sneaky [1993 film]"),
    ("kenshin tmnt", [], "Biped + Blade, Fast; Protective [1993 film]"),
    ("max winters", ["yaotl"], "Biped + Blade, Portal; Shadow; Sneaky [2007 film]"),
    ("winters' stone generals", ["stone generals"], "Biped + Swarm, Heavy, Blunt; Earth; Aggressive [2007 film]"),
    ("general aguila", ["aguila tmnt"],"Biped + Flies, Blade; Earth [2007 film stone general]"),
    ("general gato", ["gato tmnt"], "Biped + Fast, Blade; Earth [2007 film stone general]"),
    ("general serpiente", ["serpiente tmnt"],"Biped + Constrict, Pierce; Earth [2007 film stone general]"),
    ("general mono", ["mono tmnt"], "Biped + Blunt, Charge; Earth [2007 film stone general]"),
    ("eric sacks", [], "Biped + Blade, Fast; Sneaky [2014 film]"),
    ("samurai shredder", ["2014 shredder", "armored shredder"], "Biped + Blade, Shoots, Armor; Aggressive [2014 film]"),
    ("baxter stockman 2016", [], "Biped + Shoots; Fearful [2016 film]"),
    ("krang 2016", [], "Blob + Grab, Piloted; Psychic [2016 film]"),
    # ---------------------------------------------------------------- places, items, weapons
    ("tcri building", ["tcri", "t.c.r.i."], "Pillar + Portal; Psychic [TCRI: Techno-Cosmic Research Institute]"),
    ("tgri", ["techno global research industries"], "Pillar + Portal; Poison [1991 film]"),
    ("dimension x", [], "Wall + Portal; Psychic"),
    ("second time around", ["second time around shop", "april's antique shop"], "Wall + Barrier, Guard [2012]"),
    ("antonio's pizza", ["antonio's pizza-a-go-go", "pizza a go go"], "Wall + Barrier; Playful [2012]"),
    ("sword of tengu", ["the sword of tengu"], "Sword + Blade, Fast; Fire [2003]"),
    ("karai's tanto", ["karai tanto"], "Sword + Blade, Pierce, Fast"),
    ("april's tessen", ["april o'neil tessen"], "Disc + Blade, Returning [2012]"),
    ("casey's hockey mask", ["casey jones hockey mask", "goongala mask"], "Shield + Guard, Blunt"),
    ("kuro kabuto", ["the kuro kabuto", "shredder's kuro kabuto"], "Shield + Guard, Blade; Shadow [2012 Shredder's helm]"),
    ("heart of tengu", [], "Ball + Explodes; Fire [2003 relic]"),
    ("shuriken of the foot", ["foot shuriken", "foot clan shuriken"], "Disc + Blade, Swarm"),
    ("mutagen tank", ["ooze tank"], "Bomb + Explodes, Sticky, Grow; Poison"),
    ("retro mutagen gun", ["retro-mutagen blaster"], "Gun + Shoots, Shrink; Water"),
    ("krang's ooze gun", ["mutagen gun", "ooze gun"], "Gun + Shoots, Sticky, Grow; Poison"),
    ("stockman's mouser remote", ["mouser remote"], "Staff + Shoots; Electric"),
    ("foot clan emblem", ["foot emblem"], "Shield + Guard [the red foot]"),
    ("turtle shell shield", ["shell shield tmnt"], "Shield + Guard, Bounce, Returning"),
    ("sewer pipe", ["sewer pipe tmnt"], "Pillar + Barrier"),
    ("pizza box", ["pizza box tmnt"], "Disc + Blunt, Returning"),
    ("pizza tower", ["tower of pizza boxes"], "Pillar + Barrier; Playful"),
    ("ooze vat", ["mutagen vat"], "Dome + Trap, Sticky; Poison"),
    ("foot clan dojo", ["foot dojo"], "Wall + Barrier"),
    ("technodrome eye turret", ["technodrome eyeball"], "Cannon + Beam, Shoots; Electric"),
    ("rock soldier cannon", ["traag cannon"], "Cannon + Shoots, Heavy; Earth"),
    ("triceraton blaster", [], "Gun + Shoots, Fast; Fire"),
    ("kraang blaster", ["kraang gun"], "Gun + Shoots; Electric [2012]"),
    ("bebop's blaster", ["rocksteady's blaster"], "Gun + Shoots, Heavy; Fire [1987]"),
    ("hun's knuckles", ["hun brass knuckles"], "Gauntlet + Blunt, Heavy"),
    ("chrome dome's blade", [], "Sword + Blade, Fast; Electric"),
    ("tiger claw's blaster", ["tiger claw's guns"], "Gun + Shoots, Fast; Fire [2012]"),
    ("dogpound's claws", [], "Gauntlet + Blade, Heavy [2012]"),
    ("rahzar's claws", [], "Gauntlet + Blade, Fast [2012]"),
    ("baxter's mouser", ["single mouser"], "Biped + Grab, Fast [a lone Mouser]"),
    ("mousers 2012", ["2012 mousers"], "Biped + Swarm, Grab, Fast; Aggressive [2012]"),
    ("ninja turtle headband", ["turtle mask", "ninja turtle mask"], "Cape + Glide; Playful [the bandana]"),
    ("leo's blue mask", ["leonardo's mask"], "Cape + Glide; Protective"),
    ("raph's red mask", ["raphael's mask"], "Cape + Glide; Aggressive"),
    ("donnie's purple mask", ["donatello's mask"], "Cape + Glide; Fearful"),
    ("mikey's orange mask", ["michelangelo's mask"], "Cape + Glide; Playful"),
    ("booyakasha", ["boo-yah-ka-sha", "booyahkasha"], "Ramp + Launch, Bounce; Playful [2012 Mikey's cry]"),
    ("tonight i dine on turtle soup", ["turtle soup"], "Bomb + Explodes, Sticky; Water [Shredder]"),
    ("go ninja go ninja go", ["ninja rap"], "Biped + Swarm, Fast; Playful [1991 film]"),
]


def load_taken():
    taken = {}
    for f in glob.glob(os.path.join(ROOT, "assets", "icons.ron")) + glob.glob(os.path.join(ROOT, "assets", "icons", "*.ron")):
        txt = open(f, encoding="utf-8").read()
        for m in re.finditer(r"names:\s*\[(.*?)\]", txt, re.S):
            for n in re.findall(r'"((?:[^"\\]|\\.)*)"', m.group(1)):
                taken.setdefault(n.lower(), os.path.basename(f))
    for f in glob.glob(os.path.join(ROOT, "assets", "icons", "*_names.txt")):
        if os.path.basename(f) == "tmnt_names.txt":
            continue
        for line in open(f, encoding="utf-8"):
            if not line.strip() or line.startswith("#"):
                continue
            parts = [p.strip() for p in line.split("|")]
            for n in [parts[0]] + [a.strip() for a in (parts[1] if len(parts) > 1 else "").split(";")]:
                if n:
                    taken.setdefault(n.lower(), os.path.basename(f))
    pdir = os.path.join(ROOT, "assets", "powers")
    for pf in sorted(f for f in os.listdir(pdir) if f.endswith(".ron")):
        txt = open(os.path.join(pdir, pf), encoding="utf-8").read()
        for m in re.finditer(r"names:\s*\[(.*?)\]", txt, re.S):
            for n in re.findall(r'"((?:[^"\\]|\\.)*)"', m.group(1)):
                taken.setdefault(n.lower(), "powers/" + pf)
    return taken


def main():
    taken = load_taken()
    out = [
        "# Brightest Day - Teenage Mutant Ninja Turtles names (2026-09): characters, vehicles, places and items across",
        "# the 1987 cartoon + toy line, Mirage, 2003, 2012, Rise, Mutant Mayhem (2023), IDW/Archie, the games (Turtles in",
        "# Time, Tournament Fighters, Shredder's Revenge) and the films. Names and facts only. Hand-authored icons live in",
        "# tmnt.ron (the four turtles, Splinter, April, Casey, Shredder, Krang, Bebop & Rocksteady, Baxter, Technodrome...).",
        "# name | aliases (;) | Wikidata QID (blank = unresolved) | hint: Form + mechanics; element; temperament [notes]",
        "# Generated by tools/icons/tmnt/names.py (drops anything another pack already names).",
    ]
    seen, dropped = set(), []
    for name, aliases, hint in N:
        key = name.lower()
        if key in taken or key in seen:
            dropped.append("%s (%s)" % (name, taken.get(key, "dup")))
            continue
        al = []
        for a in aliases:
            a = a.lower()
            if a in taken or a in seen or a == key:
                dropped.append("alias %s of %s (%s)" % (a, name, taken.get(a, "dup")))
                continue
            al.append(a)
            seen.add(a)
        seen.add(key)
        out.append("%s | %s |  | hint: %s" % (key, "; ".join(al), hint))
    path = os.path.join(ROOT, "assets", "icons", "tmnt_names.txt")
    with open(path, "w", encoding="utf-8", newline="\n") as fh:
        fh.write("\n".join(out) + "\n")
    print("wrote %s: %d names (+aliases), dropped %d" % (path, len(out) - 6, len(dropped)))
    for d in dropped:
        print("  dropped", d)


if __name__ == "__main__":
    main()
