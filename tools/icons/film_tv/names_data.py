# name | aliases (;) | hint "Form + Mech, Mech, Element" | optional @Wikipedia title for QID lookup
DATA = r"""
## Marvel — heroes, villains, creatures, gear
Rocket Raccoon | rocket racoon; rocket | Quadruped + Shoots, Explodes | @Rocket Raccoon
Star-Lord | star lord; peter quill | Biped + Shoots, Thrust | @Star-Lord
Gamora | | Biped + Blade, Fast | @Gamora
Drax the Destroyer | drax | Biped + Blade, Charge | @Drax the Destroyer
Mantis | | Biped + Psychic | @Mantis (Marvel Comics)
Nebula | | Biped + Blade, Electric | @Nebula (character)
Yondu's arrow | yaka arrow; yondu | Spear + Homing, Returning, Pierce | @Yondu
Cosmo the Spacedog | cosmo | Quadruped + Grab, Psychic | @Cosmo the Spacedog
Milano | the milano; guardians ship | Vehicle + Flies, Piloted, Shoots
Benatar | the bowie | Vehicle + Flies, Piloted, Shoots
Ego the Living Planet | ego | Ball + Attract, DropFromSky, Heavy | @Ego the Living Planet
Thor | thor odinson | Biped + Flies, Blunt, Electric | @Thor (Marvel Comics)
Jane Foster Thor | mighty thor | Biped + Flies, Returning, Electric
Gungnir | odin's spear | Spear + Pierce, Returning, Electric | @Gungnir
Hela | | Biped + Swarm, Pierce, Shadow | @Hela (character)
Surtur | | Kaiju + Blade, Fire | @Surtur (Marvel Comics)
Fenris wolf | fenris | Quadruped + Charge, Grab
Bifrost | rainbow bridge | Platform + Portal
Twilight Sword | surtur's sword | Sword + Blade, Heavy, Fire
Jarnbjorn | thor's axe jarnbjorn | Axe + Blade, Returning
Captain America | steve rogers; cap | Biped + Guard, Ricochet | @Captain America
Sam Wilson Captain America | sam wilson | Wings + Glide, Shoots | @Sam Wilson (Marvel Cinematic Universe)
Falcon wings | falcon's wings; redwing | Wings + Glide, Shoots
Redwing drone | redwing | Bird + Flies, Shoots, Protective
Bucky Barnes | winter soldier; bucky | Gauntlet + Blunt, Grab | @Bucky Barnes
Winter Soldier's arm | vibranium arm; bucky's arm | Gauntlet + Blunt, Grab
US Agent's shield | us agent; john walker | Shield + Guard, Returning | @U.S. Agent
Iron Man Mark 42 | mark 42; mark xlii | Biped + Piloted, Beam, Flies
Iron Legion | | Biped + Swarm, Flies, Shoots
Unibeam | uni-beam | Gun + Beam, Repel
Arc reactor | arc reactor | Bomb + Explodes, Electric | @Arc reactor
Ultron | | Biped + Flies, Beam, Swarm | @Ultron
Ultron sentries | ultron drones | Biped + Swarm, Flies, Shoots
Vision | the vision | Biped + Flies, Beam | @Vision (Marvel Comics)
Scarlet Witch | wanda maximoff; wanda | Biped + Grab, Psychic | @Scarlet Witch
Agatha Harkness | agatha | Biped + Pull, Shadow | @Agatha Harkness
Doctor Strange | dr strange; stephen strange | Biped + Portal, Shoots | @Doctor Strange
Eye of Agamotto | | Disc + Repel, Psychic
Wong | | Biped + Portal, Guard | @Wong (Marvel Comics)
Dormammu | | Kaiju + Breath, Fire | @Dormammu
Mirror Dimension | | Dome + Trap, Psychic
Black Widow | natasha romanoff; widow | Biped + Fast, Electric | @Black Widow (Natasha Romanova)
Widow's Bite | widow bites | Gauntlet + Shoots, Electric
Yelena Belova | yelena | Biped + Fast, Blade | @Yelena Belova
Red Guardian | red guardian's shield | Shield + Guard, Blunt | @Red Guardian
Ghost | ava starr | Biped + Sneaky, Fast | @Ghost (Marvel Comics)
Taskmaster | | Biped + Guard, Shoots | @Taskmaster (Marvel Comics)
Sentry | robert reynolds | Biped + Flies, Charge | @Sentry (Marvel Comics)
The Void | void | Blob + Attract, Shadow
Hawkeye's quiver | hawkeye quiver | Bow + Explodes, Pull
Kate Bishop | | Bow + Ricochet, Pierce | @Kate Bishop
Falcon | falcon marvel | Wings + Glide, Shoots | @Falcon (comics)
Ant-Man and the Wasp | the wasp; wasp | Insect + Shrink, Fast, Electric | @Wasp (character)
Giant-Man Scott Lang | giant scott lang | Kaiju + Grab, Stomp, Grow
Pym tank | pym van | Vehicle + Grow, Shrink, Charge
MODOK | m.o.d.o.k. | Blob + Beam, Flies, Psychic | @MODOK
Kang the Conqueror | kang | Biped + Beam, Portal | @Kang the Conqueror
Time Variance Authority | tva; tempad | Platform + Portal
Alligator Loki | | Quadruped + Grab, Charge
Miss Minutes | | Blob + Playful, Portal
He Who Remains | | Biped + Portal
Captain Marvel | carol danvers | Biped + Flies, Beam, Fire | @Captain Marvel (Carol Danvers)
Goose the Flerken | goose; flerken | Quadruped + Grab, Constrict | @Goose (Marvel Cinematic Universe)
Ms. Marvel | kamala khan | Gauntlet + Grow, Grab | @Kamala Khan
Monica Rambeau | photon | Biped + Beam, Flies | @Monica Rambeau
Shang-Chi | | Biped + Fast, Blunt | @Shang-Chi
Ten Rings | the ten rings | Disc + Swarm, Returning, Pull
Great Protector dragon | great protector | Serpent + Flies, Water
Dweller-in-Darkness | | Kaiju + Grab, Shadow
Morris the hundun | morris | Blob + Flies, Playful
Moon Knight | marc spector; mr knight | Biped + Ricochet, Returning | @Moon Knight
Khonshu | | Biped + Flies, Shadow | @Khonshu
Crescent darts | moon knight darts | Disc + Swarm, Ricochet
Ammit | | Quadruped + Grab, Charge | @Ammit
She-Hulk | jennifer walters | Biped + Blunt, Charge | @She-Hulk
Abomination | emil blonsky | Biped + Blunt, Charge, Heavy | @Abomination (character)
Leader | the leader; samuel sterns | Biped + Psychic | @Leader (character)
Red Hulk | thaddeus ross | Biped + Charge, Blunt, Fire | @Red Hulk
Werewolf by Night | jack russell | Quadruped + Charge, Grab | @Werewolf by Night
Man-Thing | | Blob + Constrict, Fire | @Man-Thing
Blade | eric brooks | Sword + Blade, Fast | @Blade (character)
Blade's glaive | blade glaive | Disc + Blade, Returning
Morbius | | Bird + Flies, Grab | @Morbius, the Living Vampire
Kraven the Hunter | kraven | Spear + Pierce, Charge | @Kraven the Hunter
Rhino | aleksei sytsevich | Quadruped + Charge, Heavy | @Rhino (Marvel Comics)
Doctor Octopus | doc ock; otto octavius | Arachnid + Grab, Constrict | @Doctor Octopus
Doc Ock's tentacles | octavius arms | Whip + Grab, Constrict
Green Goblin | norman osborn | Vehicle + Flies, Explodes | @Green Goblin
Goblin glider | goblin glider | Vehicle + Flies, Piloted, Shoots
Pumpkin bomb | pumpkin bombs | Bomb + Explodes, Fire
Electro | max dillon | Biped + Beam, Electric | @Electro (Marvel Comics)
Sandman | flint marko | Blob + Grow, Earth | @Sandman (Marvel Comics)
Lizard | curt connors | Biped + ArticulatedTail, Charge | @Lizard (character)
Mysterio | quentin beck | Biped + Swarm, Sneaky | @Mysterio
Mysterio's drones | illusion drones | Bird + Swarm, Shoots
Vulture | adrian toomes | Wings + Glide, Grab | @Vulture (Marvel Comics)
Scorpion | mac gargan | Arachnid + ArticulatedTail, Poison | @Scorpion (Marvel Comics)
Shocker | | Gauntlet + Repel, Blunt | @Shocker (character)
Spider-Gwen | ghost-spider; gwen stacy | Biped + Fast, Pull, Sticky | @Spider-Gwen
Spider-Man 2099 | miguel o'hara | Biped + Blade, Pull | @Spider-Man 2099
Spider-Punk | hobie brown | Biped + Blunt, Playful
Spider-Ham | peter porker | Quadruped + Blunt, Playful | @Spider-Ham
Spot | the spot | Blob + Portal, Shadow | @Spot (comics)
Prowler | | Biped + Sneaky, Blade | @Prowler (comics)
Kingpin | wilson fisk | Biped + Blunt, Heavy | @Kingpin (character)
Bullseye | | Disc + Homing, Ricochet | @Bullseye (Marvel Comics)
Elektra's sai | elektra | Sword + Pierce, Fast | @Elektra (character)
Punisher | frank castle | Gun + Shoots, Heavy | @Punisher
Luke Cage | power man | Biped + Guard, Blunt | @Luke Cage
Iron Fist | danny rand | Gauntlet + Blunt, Light | @Iron Fist (character)
Jessica Jones | | Biped + Blunt, Launch | @Jessica Jones
Silver Surfer | norrin radd | Vehicle + Flies, Fast | @Silver Surfer
Mister Fantastic | reed richards | Whip + Constrict, Pull | @Mister Fantastic
Invisible Woman's force field | sue storm; invisible woman | Dome + Barrier, Repel | @Invisible Woman
H.E.R.B.I.E. | herbie | Biped + Flies, Shoots, Protective | @H.E.R.B.I.E.
Fantasticar | fantasti-car | Vehicle + Flies, Piloted
Doctor Doom | victor von doom; doom | Biped + Beam, Electric | @Doctor Doom
Doombots | doom bots | Biped + Swarm, Shoots
Mole Man | | Biped + Earth | @Mole Man
Giganto | | Kaiju + Charge, Stomp
Annihilus | | Insect + Flies, Swarm | @Annihilus
Terrax | | Axe + Blade, Earth | @Terrax
Frankie Raye | nova | Biped + Fire, Flies
Magneto | erik lehnsherr | Gauntlet + Attract, Repel, Electric | @Magneto (Marvel Comics)
Professor X | charles xavier | Biped + Psychic | @Professor X
Cerebro | | Dome + Psychic
Storm | ororo munroe | Staff + Beam, Electric, Wind | @Storm (Marvel Comics)
Jean Grey | phoenix | Bird + Flies, Fire, Psychic | @Jean Grey
Phoenix Force | dark phoenix | Bird + Flies, Breath, Fire | @Phoenix Force (comics)
Gambit | remy lebeau | Disc + Explodes, Swarm | @Gambit (Marvel Comics)
Rogue | | Biped + Flies, Grab | @Rogue (Marvel Comics)
Colossus | piotr rasputin | Biped + Heavy, Blunt | @Colossus (character)
Nightcrawler | kurt wagner | Biped + Portal, ArticulatedTail | @Nightcrawler (character)
Iceman | bobby drake | Ramp + Launch, Ice | @Iceman (Marvel Comics)
Beast | hank mccoy | Biped + Charge, Grab | @Beast (Marvel Comics)
Juggernaut | cain marko | Biped + Charge, Heavy | @Juggernaut (character)
Sabretooth | | Gauntlet + Blade, Charge | @Sabretooth (character)
Mystique | | Biped + Sneaky | @Mystique (character)
Apocalypse | en sabah nur | Kaiju + Beam | @Apocalypse (character)
Blackbird | x-jet | Vehicle + Flies, Piloted | @X-Jet
Danger Room | | Cage + Trap, Shoots
Cable | nathan summers | Gun + Shoots, Heavy | @Cable (character)
Domino | | Gun + Ricochet | @Domino (character)
Dogpool | | Quadruped + Playful, Charge
Juggernaut helmet | | Shield + Charge, Heavy
Silver Samurai | | Sword + Blade, Electric | @Silver Samurai
Omega Red | | Whip + Constrict, Poison | @Omega Red
Thanos | mad titan | Kaiju + Blunt, Grab | @Thanos
Thanos' ship | sanctuary ii | Vehicle + DropFromSky, Shoots
Outriders | | Quadruped + Swarm, Charge
Chitauri | chitauri soldiers | Biped + Swarm, Shoots | @Chitauri
Chitauri leviathan | leviathan | Serpent + Flies, Charge, Heavy
Ebony Maw | | Biped + Pierce, Psychic | @Ebony Maw
Proxima Midnight's spear | proxima midnight | Spear + Pierce, Returning, Electric
Corvus Glaive | glaive | Spear + Blade | @Corvus Glaive
Cull Obsidian | | Biped + Blunt, Heavy | @Cull Obsidian
Knull | king in black | Kaiju + Shadow, Swarm | @Knull (character)
Carnage symbiote | cletus kasady | Blob + Blade, Constrict | @Carnage (character)
Toxin | | Blob + Grab, Sticky
Kree Sentry | kree sentry | Kaiju + Beam, Stomp
Ronan the Accuser | ronan | Hammer + Blunt, Beam | @Ronan the Accuser
Universal Weapon | ronan's hammer | Hammer + Blunt, Beam
Collector's museum | | Cage + Trap
Howard the Duck | | Bird + Playful | @Howard the Duck
Squirrel Girl | | Quadruped + Swarm, Playful | @Squirrel Girl
Devil Dinosaur | | Kaiju + Grab, Stomp | @Devil Dinosaur
Moon Girl | lunella lafayette | Biped + Shoots | @Moon Girl
Fin Fang Foom | | Kaiju + Breath, Flies, Fire | @Fin Fang Foom
Tiboro | | Biped + Shadow
Ironheart | riri williams | Biped + Flies, Piloted, Beam | @Ironheart (character)
Blue Marvel | | Biped + Flies, Beam
Captain Carter | peggy carter shield | Shield + Guard, Returning | @Captain Carter
Zombie Captain America | marvel zombies | Biped + Swarm, Chaotic
Eternals | eternals | Biped + Beam, Flies | @Eternals (comics)
Celestial Tiamut | tiamut; celestial | Kaiju + Stomp, Beam
Deviants | | Quadruped + Swarm, Charge
Arishem | | Kaiju + Beam
Namor | | Spear + Pierce, Water, Flies | @Namor
Talokan whale | | Fish + Charge, Water
Shuri's gauntlets | panther gauntlets | Gauntlet + Shoots, Electric
Royal Talon Fighter | talon fighter | Vehicle + Flies, Piloted
M'Baku | | Staff + Blunt | @M'Baku
Okoye's spear | okoye | Spear + Pierce, Returning | @Okoye
War rhino | wakandan rhino | Quadruped + Charge, Heavy, Piloted
Captain America's motorcycle | cap motorcycle | Vehicle + Fast, Piloted
Sanctum Sanctorum | | Wall + Barrier, Portal
Wand of Watoomb | | Staff + Beam, Wind
Crimson Bands of Cyttorak | crimson bands | Whip + Constrict, Link
Casket of Ancient Winters | | Bomb + Explodes, Ice
Darkhold | | Bomb + Shadow
Tesseract | space stone | Ball + Portal | @Tesseract (Marvel Cinematic Universe)
Aether | reality stone | Blob + Swarm, Shadow
Orb | power stone | Ball + Explodes, Psychic
Stark Industries missile | jericho missile | Rocket + Explodes, Homing, Swarm
Pym Particle van | quantum tunnel | Platform + Portal, Shrink
Quantum Realm | | Platform + Portal, Shrink
## DC — heroes, villains, gear, vehicles
Batman | bruce wayne; the batman | Biped + Glide, Sneaky | @Batman
Batcave | | Dome + Barrier | @Batcave
Batcycle | batpod; bat-pod | Vehicle + Fast, Piloted, Shoots | @Batcycle
Batboat | | Vehicle + Fast, Water
Batcopter | | Vehicle + Flies, Rotor
Batman's cape | bat cape | Cape + Glide
Utility belt | batman's utility belt | Bomb + Explodes, Sticky
Robin | dick grayson; damian wayne | Staff + Blunt, Fast | @Robin (character)
Nightwing | escrima sticks | Staff + Blunt, Electric | @Nightwing
Batgirl | barbara gordon | Disc + Returning | @Batgirl
Red Hood | jason todd | Gun + Shoots | @Red Hood
Alfred | alfred pennyworth | Biped + Protective | @Alfred Pennyworth
The Joker | joker | Biped + Chaotic, Explodes | @Joker (character)
Joker's bang flag gun | bang gun | Gun + Launch, Playful
Joker's teeth | chattering teeth | Arachnid + Swarm, Explodes
Harley Quinn's bat | harley bat | Hammer + Blunt, Playful
Two-Face | harvey dent | Gun + Shoots | @Two-Face
Scarecrow | jonathan crane | Bomb + Explodes, Poison | @Scarecrow (DC Comics)
Riddler | edward nygma | Staff + Trap | @Riddler
Bane | | Biped + Charge, Heavy | @Bane (DC Comics)
Killer Croc | | Biped + Grab, Charge | @Killer Croc
Clayface | | Blob + Grow, Earth | @Clayface
Man-Bat | | Bird + Flies, Grab | @Man-Bat
Catwoman | selina kyle | Whip + Pull, Fast | @Catwoman
Catwoman's whip | cat whip | Whip + Pull
Ra's al Ghul | | Sword + Blade | @Ra's al Ghul
Lazarus Pit | | Dome + Barrier
Deathstroke | slade wilson | Sword + Blade, Shoots | @Deathstroke
Hush | | Gun + Sneaky
Mad Hatter | | Biped + Psychic
Ventriloquist | scarface | Biped + Shoots
Firefly | garfield lynns | Jetpack + Thrust, Fire | @Firefly (DC Comics)
Court of Owls | talons | Biped + Swarm, Blade
Mr. Freeze's suit | freeze suit | Biped + Beam, Ice, Heavy
Superman's cape | superman cape | Cape + Glide, Flies
Heat vision | superman heat vision | Gun + Beam, Fire
Freeze breath | super breath | Staff + Breath, Ice
Fortress of Solitude | | Pillar + Barrier, Ice | @Fortress of Solitude
Lex Luthor | | Biped + Piloted, Beam | @Lex Luthor
Lex Luthor's warsuit | luthor power armor | Biped + Piloted, Beam, Flies
Brainiac | | Biped + Grab, Psychic | @Brainiac (character)
Brainiac's ship | skull ship | Vehicle + DropFromSky, Grab
Bottle City of Kandor | kandor | Dome + Shrink, Trap
Zod | general zod | Biped + Flies, Beam | @General Zod
Phantom Zone | phantom zone projector | Cage + Trap, Portal | @Phantom Zone
Parasite | | Blob + Grab, Poison | @Parasite (DC Comics)
Metallo | | Biped + Beam, Poison | @Metallo
Bizarro | | Biped + Flies, Ice | @Bizarro
Mr. Mxyzptlk | mxyzptlk | Blob + Chaotic, Psychic | @Mister Mxyzptlk
Toyman | | Bomb + Walks, Explodes | @Toyman
Ultraman | | Biped + Flies, Beam
The Engineer | engineer | Blob + Blade, Sticky
Hammer of Boravia | | Hammer + Blunt, Heavy
Metamorpho | rex mason | Blob + Poison, Earth | @Metamorpho
Guy Gardner's power ring | | Gauntlet + Blunt
Hawkgirl's wings | nth metal wings | Wings + Glide, Blunt
Mister Terrific | michael holt | Ball + Swarm, Homing | @Mister Terrific
Justice Gang | | Biped + Swarm
Superman robots | superman robot | Biped + Swarm, Flies
Supergirl's cape | | Cape + Glide
Streaky the Supercat | streaky | Quadruped + Flies, Playful | @Streaky the Supercat
Comet the Super-Horse | comet | Quadruped + Flies, Piloted
Wonder Woman's sword | godkiller | Sword + Blade, Electric
Wonder Woman's shield | amazon shield | Shield + Guard, Ricochet
Invisible Jet | wonder woman's jet | Vehicle + Flies, Piloted, Sneaky | @Invisible Plane
Cheetah | barbara minerva | Quadruped + Fast, Blade | @Cheetah (character)
Ares | | Biped + Blade | @Ares (DC Comics)
Aquaman | arthur curry | Spear + Pierce, Water | @Aquaman
Black Manta | | Gun + Beam, Water | @Black Manta
Black Manta's helmet | manta helmet | Gun + Beam
Karathen | | Kaiju + Grab, Water
Trench | the trench | Fish + Swarm, Aggressive
Topo | | Serpent + Constrict, Water
Mera | | Fish + Water, Pull | @Mera (character)
The Flash's lightning | speed force lightning | Staff + Beam, Electric
Reverse-Flash | eobard thawne | Biped + Fast, Charge | @Eobard Thawne
Captain Cold | cold gun | Gun + Beam, Ice | @Captain Cold
Heat Wave | heat gun | Gun + Beam, Fire | @Heat Wave (character)
Gorilla Grodd | grodd | Biped + Charge, Psychic | @Gorilla Grodd
Cosmic Treadmill | | Platform + Portal
Cyborg | victor stone | Biped + Shoots, Repel | @Cyborg (DC Comics)
Martian Manhunter | j'onn j'onzz | Biped + Flies, Psychic | @Martian Manhunter
Shazam | billy batson; captain marvel shazam | Biped + Flies, Electric | @Shazam (DC Comics)
Black Adam | teth-adam | Biped + Flies, Electric | @Black Adam
Blue Beetle | jaime reyes | Biped + Flies, Shoots | @Blue Beetle (Jaime Reyes)
Blue Beetle scarab | khaji-da | Gauntlet + Beam, Blade
Booster Gold | | Biped + Flies, Shoots | @Booster Gold
Skeets | | Ball + Flies, Protective
Firestorm | | Biped + Flies, Fire | @Firestorm (character)
Zatanna | | Staff + Portal, Psychic | @Zatanna
John Constantine | constantine | Biped + Fire, Sneaky | @John Constantine
Swamp Thing | | Biped + Constrict, Earth | @Swamp Thing
Raven | | Bird + Flies, Shadow | @Raven (DC Comics)
Beast Boy | | Quadruped + Charge, Playful | @Beast Boy
Starfire | | Biped + Flies, Fire | @Starfire (Koriand'r)
Trigon | | Kaiju + Beam, Fire | @Trigon
Plastic Man | | Whip + Constrict, Bounce | @Plastic Man
Atom | ray palmer | Biped + Shrink, Fast | @Atom (Ray Palmer)
Doctor Fate | helmet of fate | Staff + Beam, Psychic | @Doctor Fate
Hawkman | | Wings + Glide, Blunt | @Hawkman
Peacemaker's eagle | eagly | Bird + Flies, Aggressive
Vigilante | | Gun + Shoots
Bloodsport | | Gun + Swarm, Explodes | @Bloodsport (character)
Polka-Dot Man | polka dot man | Ball + Explodes, Swarm | @Polka-Dot Man
Ratcatcher 2 | ratcatcher | Quadruped + Swarm, Sneaky
Sebastian the rat | | Quadruped + Playful, Sneaky
Weasel | | Quadruped + Chaotic
Captain Boomerang | | Disc + Returning, Explodes | @Captain Boomerang
El Diablo | | Biped + Fire, Breath | @El Diablo (DC Comics)
Enchantress | | Biped + Portal, Shadow | @Enchantress (DC Comics)
Deadshot | | Gun + Homing, Shoots | @Deadshot
Amanda Waller | | Biped + Sneaky
Darkseid | | Kaiju + Beam, Fire | @Darkseid
Parademons | parademon | Bird + Swarm, Flies | @Parademon
Steppenwolf | | Axe + Blade, Electric | @Steppenwolf (character)
Apokolips fire pits | | Dome + Fire
Mister Miracle | | Cage + Trap, Portal | @Mister Miracle
Big Barda's Mega-Rod | mega rod | Staff + Blunt, Beam
Lobo | | Hammer + Chains, Charge | @Lobo (DC Comics)
Lobo's hook and chain | lobo chain | Whip + Pull, Link
Space Hog | lobo's bike | Vehicle + Fast, Piloted
Sinestro's ring | yellow power ring | Gauntlet + Pierce, Shadow
Arkillo | | Biped + Charge, Aggressive
Mongul | | Biped + Charge, Heavy | @Mongul
Black Hand | | Biped + Shadow
Hector Hammond | | Blob + Psychic
Krona | | Biped + Beam
Guardians of the Universe | guardians of oa | Biped + Swarm, Psychic | @Guardians of the Universe
Oa | planet oa | Ball + DropFromSky, Heavy
Salaak | | Biped + Guard
Tomar-Re | | Bird + Flies
Abin Sur | | Biped + Flies
Jessica Cruz | | Biped + Guard, Barrier | @Jessica Cruz
Simon Baz | | Gun + Shoots | @Simon Baz
Alan Scott | green lantern alan scott | Biped + Flies, Shadow | @Alan Scott
Sodam Yat | | Biped + Flies, Beam
Saint Walker | | Biped + Protective
Carol Ferris | | Biped + Flies
Arisia | | Biped + Flies
Manhunters | manhunter robots | Biped + Swarm, Shoots
Relic | | Kaiju + Attract
Nekron's scythe | | Axe + Blade, Shadow
Butcher | red lantern entity | Quadruped + Charge, Fire
Ophidian | | Serpent + Constrict
Predator entity | violet entity | Serpent + Flies, Psychic
Adara | | Bird + Flies
Proselyte | | Blob + Psychic
Ion entity | | Serpent + Flies, Beam
Doomsday's bone spikes | | Spear + Pierce, Earth
Swamp monster | | Blob + Constrict, Earth
Solomon Grundy | | Biped + Blunt, Heavy | @Solomon Grundy
Atomic Skull | | Biped + Atomic | @Atomic Skull
Gotham City blimp | gotham blimp | Vehicle + Flies, DropFromSky
## Star Wars — characters, creatures, vehicles, gear
Luke Skywalker | | Sword + Blade, Psychic | @Luke Skywalker
Darth Vader's lightsaber | vader's lightsaber | Sword + Blade, Heavy
Obi-Wan Kenobi | obi wan; ben kenobi | Sword + Blade, Guard | @Obi-Wan Kenobi
Anakin Skywalker | | Sword + Blade, Fast | @Anakin Skywalker
Ahsoka Tano | ahsoka | Sword + Blade, Fast | @Ahsoka Tano
Ahsoka's white lightsabers | ahsoka's sabers | Sword + Blade, Fast
Kylo Ren | kylo ren's lightsaber; crossguard lightsaber | Sword + Blade, Fire | @Kylo Ren
Rey | | Staff + Blunt, Blade | @Rey (Star Wars)
Mace Windu | purple lightsaber | Sword + Blade | @Mace Windu
Count Dooku | curved lightsaber | Sword + Blade, Electric | @Count Dooku
General Grievous | grievous | Arachnid + Blade, Spin | @General Grievous
Emperor Palpatine | the emperor | Gauntlet + Beam, Electric | @Palpatine
Inquisitor lightsaber | spinning lightsaber | Disc + Blade, Spin
Din Djarin | mando; the mandalorian | Biped + Shoots, Thrust | @Din Djarin
Boba Fett | | Jetpack + Thrust, Shoots | @Boba Fett
Jango Fett | | Gun + Shoots, Thrust | @Jango Fett
Slave I | firespray | Vehicle + Flies, Piloted, Shoots
Razor Crest | | Vehicle + Flies, Piloted, Shoots
Amban rifle | disintegration rifle | Gun + Beam
Beskar | beskar armor | Shield + Guard, Heavy
IG-11 | ig 11 | Biped + Spin, Shoots
IG-12 | | Vehicle + Piloted, Shoots
Moff Gideon | | Biped + Shoots
Dark troopers | | Biped + Swarm, Flies
Blurrg | | Quadruped + Charge, Piloted
Mudhorn | | Quadruped + Charge, Heavy
Krayt dragon | | Serpent + Grab, Breath
Anzellans | | Biped + Swarm, Playful
Han Solo | | Gun + Shoots | @Han Solo
DL-44 blaster | han's blaster | Gun + Shoots
Chewbacca | | Biped + Charge, Grab | @Chewbacca
Stormtroopers | stormtrooper | Biped + Swarm, Shoots | @Stormtrooper (Star Wars)
Clone troopers | clone trooper | Biped + Swarm, Shoots | @Clone trooper
Battle droids | b1 battle droid | Biped + Swarm, Shoots
Droideka | destroyer droid | Ball + Guard, Shoots | @Droideka
Buzz droids | | Insect + Swarm, Sticky
Probe droid | | Arachnid + Flies, Shoots
C-3PO | c3po; threepio | Biped + Fearful | @C-3PO
K-2SO | | Biped + Blunt, Protective | @K-2SO
Chopper | c1-10p | Vehicle + Electric, Playful
Ewoks | ewok | Biped + Swarm, Playful | @Ewok
Wicket | | Biped + Playful
Jawas | jawa | Biped + Swarm, Electric | @Jawa
Sandcrawler | | Vehicle + Heavy, Treads
Tusken Raiders | sand people | Staff + Blunt | @Tusken Raiders
Gaffi stick | | Staff + Blunt
Bantha | | Quadruped + Charge, Heavy | @Bantha
Wampa | | Biped + Grab, Ice | @Wampa
Dewback | | Quadruped + Piloted
Sarlacc tentacle | | Whip + Grab, Pull
Rathtar | rathtars | Blob + Grab, Constrict | @Rathtar
Exogorth | space slug | Serpent + Grab
Dianoga | | Serpent + Constrict, Water
Acklay | | Arachnid + Pierce, Charge
Nexu | | Quadruped + Fast, Blade
Reek | | Quadruped + Charge
Jabba the Hutt | jabba | Blob + Lazy, Heavy | @Jabba the Hutt
Jabba's sail barge | sail barge | Vehicle + Flies, Shoots
Snowspeeder | | Vehicle + Flies, Link
Harpoon tow cable | | Gun + Link, Pull
A-wing | | Vehicle + Fast, Flies
B-wing | | Vehicle + Flies, Shoots
Y-wing | | Vehicle + Flies, Explodes
TIE Interceptor | | Vehicle + Fast, Flies
TIE Advanced | vader's tie | Vehicle + Flies, Shoots
Imperial shuttle | lambda shuttle | Vehicle + Flies, Guard
Naboo starfighter | | Vehicle + Fast, Flies
Jedi starfighter | | Vehicle + Fast, Flies
Republic gunship | laat | Vehicle + Flies, Shoots
AT-TE | | Arachnid + Shoots, Heavy
AT-ACT | | Quadruped + Heavy, Stomp
Juggernaut tank | | Vehicle + Heavy, Treads
Starkiller Base | | Ball + Beam, DropFromSky
Death Star II | | Ball + Beam, DropFromSky
Super Star Destroyer | executor | Vehicle + DropFromSky, Heavy
Holocron | | Ball + Psychic
Kyber crystal | | Boulder + Light
Lightwhip | | Whip + Blade, Pull
Electrostaff | | Staff + Blunt, Electric
Sith Eternal fleet | | Vehicle + Swarm, Flies
Andor | cassian andor | Gun + Sneaky
Grogu's Force shield | | Dome + Barrier
Loth-cat | | Quadruped + Playful
Purrgil | | Fish + Flies, Heavy
## Lord of the Rings / The Hobbit / Rings of Power
Glamdring | gandalf's sword | Sword + Blade, Light | @Glamdring
Orcrist | thorin's sword | Sword + Blade
Frodo | frodo baggins | Biped + Sneaky | @Frodo Baggins
Samwise | sam gamgee | Biped + Protective
Gollum | smeagol | Biped + Sneaky, Grab | @Gollum
Legolas' bow | legolas | Bow + Fast, Pierce | @Legolas
Gimli's axe | gimli | Axe + Blade, Heavy | @Gimli (Middle-earth)
Aragorn | strider | Sword + Blade | @Aragorn
Boromir's horn | horn of gondor | Staff + Repel, Wind
Sauron | | Kaiju + Blunt, Shadow | @Sauron
Sauron's mace | | Hammer + Blunt, Heavy, Shadow
Eye of Sauron | | Pillar + Beam, Fire
Saruman | | Staff + Beam, Wind | @Saruman
Palantír | palantir; seeing stone | Ball + Psychic | @Palantír
Witch-king's flail | morgul flail | Hammer + Blunt, Shadow
Morgul blade | | Sword + Blade, Poison
Uruk-hai | uruk hai | Biped + Swarm, Blade | @Uruk-hai
Orcs | orc army | Biped + Swarm, Aggressive
Cave troll | | Biped + Blunt, Heavy
Mountain troll | | Biped + Charge, Heavy
Wargs | warg | Quadruped + Swarm, Charge | @Warg
Watcher in the Water | | Serpent + Constrict, Water
Barrow-wight | | Biped + Shadow
Trolls of the Hobbit | stone trolls | Biped + Heavy, Earth
Beorn | | Quadruped + Charge, Heavy | @Beorn
Mirkwood spiders | | Arachnid + Swarm, Sticky
Azog | | Hammer + Blunt | @Azog
Thorin Oakenshield | | Shield + Guard | @Thorin Oakenshield
Arkenstone | | Ball + Light
Lake-town | | Platform + Water
Black Arrow | | Spear + Pierce
Were-worms | | Serpent + Charge, Earth
Galadriel | | Biped + Light, Psychic | @Galadriel
Phial of Galadriel | star-glass | Bomb + Light, Repel
Mithril | mithril coat | Shield + Guard
Mount Doom | | Dome + Fire
Rohirrim | riders of rohan | Quadruped + Swarm, Charge, Piloted
Shadowfax | | Quadruped + Fast, Piloted | @Shadowfax
Gwaihir | | Bird + Flies, Grab
Radagast's sled | rhosgobel rabbits | Vehicle + Fast, Piloted
Dragon of the North | | Kaiju + Breath, Fire
Glaurung | | Serpent + Breath, Fire | @Glaurung
Ancalagon the Black | | Kaiju + Flies, Breath, Fire | @Ancalagon
Tom Bombadil | | Biped + Playful
Durin's Bane whip | balrog whip | Whip + Pull, Fire
Númenor fleet | numenorean ship | Vehicle + Water
Adar | | Sword + Blade, Shadow
Halbrand | | Hammer + Blunt
Stranger | the stranger | Biped + Psychic, Light
Snow troll | | Biped + Ice, Heavy
Hill-troll | | Biped + Blunt, Heavy
## Harry Potter / Wizarding World
Harry Potter | the boy who lived | Staff + Beam, Shoots | @Harry Potter (character)
Hermione Granger | hermione | Staff + Shoots, Trap | @Hermione Granger
Ron Weasley | | Staff + Shoots, Fearful | @Ron Weasley
Dumbledore | albus dumbledore | Staff + Beam, Fire | @Albus Dumbledore
Voldemort | he who must not be named | Staff + Beam, Poison | @Lord Voldemort
Nagini | | Serpent + Constrict, Poison | @Nagini
Hagrid | | Biped + Heavy, Protective | @Rubeus Hagrid
Hagrid's motorbike | flying motorcycle | Vehicle + Flies, Piloted
Flying Ford Anglia | ford anglia | Vehicle + Flies, Piloted
Knight Bus | | Vehicle + Fast, Charge
Aragog | | Arachnid + Grab, Sticky | @Aragog
Acromantula | | Arachnid + Swarm, Sticky
Norbert | norberta | Quadruped + Breath, Fire
Buckbeak | hippogriff | Bird + Flies, Piloted | @Buckbeak
Hedwig | | Bird + Flies | @Hedwig
Fluffy | three-headed dog | Quadruped + Grab, Charge
Troll in the dungeon | mountain troll hogwarts | Biped + Blunt, Heavy
Cornish pixies | pixies | Bird + Swarm, Playful
Thestral | | Bird + Flies, Shadow
Giant squid | | Serpent + Constrict, Water
Grindylow | | Fish + Grab, Water
Inferi | | Biped + Swarm, Water
Sword of Gryffindor | | Sword + Blade | @Sword of Gryffindor
Sorting Hat | | Blob + Psychic | @Sorting Hat
Marauder's Map | | Platform + Sneaky
Time-Turner | time turner | Disc + Portal
Horcrux | | Ball + Shadow
Resurrection Stone | | Ball + Shadow
Philosopher's Stone | sorcerer's stone | Boulder + Fire
Remembrall | | Ball + Returning
Howler | | Bird + Repel, Fire
Deluminator | put-outer | Gauntlet + Attract, Light
Vanishing Cabinet | | Wall + Portal
Portkey | | Ball + Portal
Floo powder | floo network | Platform + Portal, Fire
Fiendfyre | | Serpent + Breath, Fire
Wingardium Leviosa | levitation charm | Staff + SlowFall, Psychic
Expelliarmus | disarming charm | Staff + Repel
Stupefy | stunning spell | Staff + Shoots
Avada Kedavra | killing curse | Staff + Beam, Poison
Protego | shield charm | Dome + Barrier
Lumos | | Staff + Beam, Light
Accio | summoning charm | Staff + Pull
Incendio | | Staff + Breath, Fire
Sectumsempra | | Staff + Blade
Petrificus Totalus | | Staff + Trap
Niffler | | Quadruped + Grab, Sneaky
Occamy | | Serpent + Flies, Grow
Bowtruckle | pickett | Biped + Playful
Swooping Evil | | Insect + Flies, Grab
Erumpent | | Quadruped + Charge, Explodes
Thunderbird | fantastic beasts thunderbird | Bird + Flies, Electric
Obscurus | | Blob + Explodes, Shadow
Quidditch hoops | | Pillar + Barrier
## Monsterverse / Toho / kaiju & giant monsters
Skar King | | Biped + Pull, Charge, Ice | @Skar King
Shimo | | Kaiju + Breath, Ice
Suko | | Biped + Playful
Mothra larva | | Serpent + Sticky
Scylla titan | | Arachnid + Grab
Behemoth | | Kaiju + Charge, Heavy
Methuselah | | Kaiju + Stomp, Earth
Muto | m.u.t.o.; mutos | Insect + Flies, Electric
Skullcrawler | skull crawler | Serpent + Grab, Charge
Camazotz | | Bird + Flies, Grab
Warbat | | Bird + Flies, Charge
Anguirus | | Quadruped + Charge, Spikes
Gamera | | Kaiju + Flies, Spin, Fire | @Gamera
Biollante | | Kaiju + Constrict, Poison | @Biollante
Destoroyah | | Kaiju + Flies, Beam | @Destoroyah
Hedorah | smog monster | Blob + Poison, Flies | @Hedorah
Gigan | | Kaiju + Blade, Spin | @Gigan
Megalon | | Insect + Drill, Charge
Space Godzilla | spacegodzilla | Kaiju + Beam, Ice
Minilla | minya | Kaiju + Playful
Godzilla Earth | | Kaiju + Beam, Heavy
Legendary Godzilla | | Kaiju + Breath, Atomic
Heisei Godzilla | burning godzilla | Kaiju + Breath, Fire
Mechani-Kong | | Kaiju + Piloted, Grab
Kong's BEAST glove | beast glove | Gauntlet + Blunt, Electric
Cloverfield monster | clover | Kaiju + Stomp, Chaotic
Rampage George | george the albino gorilla | Biped + Charge, Grab
Rampage wolf | ralph the wolf | Quadruped + Flies, Charge
Rampage crocodile | lizzie | Kaiju + Grab, Charge
The Blob | the blob 1958 | Blob + Grab, Constrict | @The Blob
Graboids | graboid; tremors worm | Serpent + Grab, Earth
Shriekers | | Biped + Swarm
Ass-blaster | | Bird + Thrust, Fire
Kraken | release the kraken | Serpent + Constrict, Water
Medusa | | Biped + Trap, Earth
Talos | bronze giant | Kaiju + Stomp, Heavy
Pegasus | | Quadruped + Flies, Piloted
Harpies | | Bird + Swarm, Grab
Hydra | | Serpent + Swarm, Grab
Minotaur | | Biped + Charge, Heavy
Ymir | 20 million miles to earth | Biped + Grab
Rhedosaurus | | Kaiju + Stomp
Them! ants | giant ants | Insect + Swarm, Poison
Mars Attacks martians | martians | Biped + Swarm, Shoots
Martian tripod | war of the worlds tripod | Arachnid + Beam, Heavy
Independence Day mothership | alien mothership | Vehicle + Beam, DropFromSky
City destroyer | | Vehicle + Beam, DropFromSky
Arachnids | starship troopers bugs | Arachnid + Swarm, Charge
Tanker bug | | Arachnid + Breath, Fire
Plasma bug | | Arachnid + Shoots, Explodes
Mimic | edge of tomorrow mimic | Serpent + Fast, Swarm
Mech suit | exosuit | Biped + Piloted, Shoots
## Jurassic Park / Jurassic World
Blue | blue raptor | Biped + Fast, Charge, Protective
Delta raptor | | Biped + Fast, Sneaky
Indoraptor | | Biped + Sneaky, Grab
Spinosaurus | | Kaiju + Grab, Water
Giganotosaurus | | Kaiju + Charge, Stomp
Therizinosaurus | | Biped + Blade, Charge
Triceratops | | Quadruped + Charge, Heavy
Stegosaurus | | Quadruped + ArticulatedTail, Spikes
Ankylosaurus | | Quadruped + ArticulatedTail, Heavy
Brachiosaurus | | Kaiju + Stomp, Lazy
Dilophosaurus | spitter | Biped + Breath, Poison
Pachycephalosaurus | | Biped + Charge, Blunt
Parasaurolophus | | Quadruped + Repel, Wind
Gallimimus | | Biped + Swarm, Fast
Compsognathus | compies | Biped + Swarm, Fast
Carnotaurus | | Biped + Charge, Heavy
Baryonyx | | Biped + Grab, Water
Dimorphodon | | Bird + Swarm, Flies
Mutadon | | Bird + Flies, Grab
Titanosaurus | | Kaiju + Stomp, Heavy
Gyrosphere | | Ball + Piloted, Bounce
Jurassic Park jeep | explorer | Vehicle + Charge, Piloted
Amber mosquito | | Insect + Swarm, Poison
## Pacific Rim / mechs
Striker Eureka | | Kaiju + Piloted, Shoots
Crimson Typhoon | | Kaiju + Piloted, Blade, Spin
Cherno Alpha | | Kaiju + Piloted, Heavy, Blunt
Coyote Tango | | Kaiju + Piloted, Shoots
Obsidian Fury | | Kaiju + Piloted, Blade
Leatherback | | Kaiju + Electric, Charge
Raiju | | Kaiju + Charge, Fast
Onibaba | | Arachnid + Heavy
Slattern | | Kaiju + ArticulatedTail, Heavy
Otachi | | Kaiju + Flies, Poison
Kaiju Blue | | Blob + Poison, Sticky
Chain sword | gipsy danger sword | Sword + Blade, Link
Elbow rocket | rocket punch | Gauntlet + Thrust, Blunt
Plasmacaster | | Cannon + Shoots, Fire
## Alien / Predator
Alien Queen | xenomorph queen | Kaiju + Grab, ArticulatedTail | @Alien Queen
Chestburster | | Serpent + Charge, Fast
Neomorph | | Biped + Fast, Aggressive
Deacon | | Biped + Grab
Power loader | | Biped + Piloted, Grab | @Power loader
Pulse rifle | m41a pulse rifle | Gun + Shoots, Explodes | @M41A pulse rifle
Motion tracker | | Pillar + Homing
Smart gun | | Gun + Homing, Shoots
Colonial marines dropship | cheyenne dropship | Vehicle + Flies, Shoots
Weyland-Yutani synthetic | synthetic | Biped + Grab
Engineer | alien engineer | Biped + Heavy
Predator wrist blades | predator blades | Gauntlet + Blade
Combistick | predator spear | Spear + Pierce, Returning
Predator shuriken | | Disc + Blade, Returning
Predator net gun | net gun | Gun + Trap, Sticky
Predator hounds | | Quadruped + Swarm, Charge
Dek | predator badlands dek | Biped + Blade, Charge
Thia | | Biped + Grab
Feral Predator | | Biped + Charge
## Stranger Things / Wednesday / Squid Game / The Boys / Invincible
Eleven's nosebleed blast | eleven blast | Gauntlet + Repel, Psychic
Mind Flayer particles | | Blob + Swarm, Shadow
Demobats | demo bats | Bird + Swarm, Constrict
Demo-slug | slug | Serpent + Grab
Upside Down | | Platform + Portal, Shadow
Hawkins Lab gate | the gate | Platform + Portal, Shadow
Hopper's shotgun | | Gun + Shoots, Repel
Steve's nail bat | nail bat | Hammer + Blunt, Spikes
Eddie's guitar | eddie munson guitar | Hammer + Blunt, Repel
Dustin's slingshot | lucas slingshot | Bow + Shoots
Eggos | eggo waffles | Disc + Swarm, Playful
Starcourt Mall | | Wall + Barrier
Russian Demogorgon | | Biped + Grab, Ice
Hyde | wednesday hyde | Biped + Charge, Aggressive
Wednesday's cello | | Staff + Repel
Uncle Fester | | Gauntlet + Beam, Electric
Cousin Itt | | Blob + Playful
Addams Family mansion | | Wall + Barrier, Shadow
Squid Game guards' rifles | | Gun + Shoots
Dalgona cookie | dalgona | Disc + Blade
Squid Game piggy bank | | Ball + Heavy, DropFromSky
Glass bridge | | Platform + Trap
Tug of war rope | | Whip + Link, Pull
Soldier Boy | | Shield + Guard, Beam
Starlight | | Gauntlet + Beam, Light
Queen Maeve | | Sword + Blade, Guard
A-Train | | Biped + Fast, Charge
Black Noir | | Sword + Sneaky, Blade
The Deep | | Fish + Swarm, Water
Kimiko | | Biped + Aggressive, Grab
Compound V | | Bomb + Grow, Poison
Omni-Man | | Biped + Flies, Charge, Blunt
Atom Eve | | Dome + Barrier, Psychic
Allen the Alien | allen | Biped + Blunt
Battle Beast | | Axe + Blade, Charge
Thragg | | Biped + Charge, Heavy
Viltrumites | | Biped + Swarm, Flies
Monster Girl | | Biped + Grow, Charge
Rex Splode | | Ball + Explodes, Swarm
Dupli-Kate | | Biped + Swarm
Bulletproof | | Biped + Flies, Guard
Titan | | Biped + Earth, Heavy
Angstrom Levy | | Platform + Portal
Conquest | | Biped + Blade, Charge
## Arcane / KPop Demon Hunters / One-offs
Pow-Pow | jinx minigun | Gun + Fast, Shoots
Zapper | jinx zapper | Gun + Beam, Electric
Caitlyn's rifle | caitlyn | Gun + Pierce, Shoots
Ekko's bat | ekko | Hammer + Blunt, Fast
Z-Drive | ekko z-drive | Disc + Portal
Warwick | | Quadruped + Charge, Aggressive
Viktor | | Staff + Beam, Electric
Hexcore | | Ball + Electric, Attract
Shimmer | | Blob + Poison, Grow
Hexgates | hexgate | Platform + Portal
Singed | | Biped + Poison
Sevika's arm | sevika | Gauntlet + Blade, Shoots
Mel Medarda | | Dome + Barrier, Light
Huntrix | huntr/x | Biped + Swarm, Psychic
Rumi | | Sword + Blade
Mira | | Spear + Blade
Zoey | | Disc + Swarm
Saja Boys | saja boys | Biped + Swarm, Shadow
Jinu | | Biped + Sneaky, Shadow
Gwi-Ma | gwi ma | Kaiju + Breath, Shadow
Sussie | kpop demon hunters magpie | Bird + Flies, Playful
## Animation — Pixar
Woody | sheriff woody | Whip + Pull, Link | @Sheriff Woody
Woody's lasso | woody lasso | Whip + Pull
Jessie | | Biped + Fast, Playful
Bullseye | toy story horse | Quadruped + Fast, Piloted
Rex | toy story rex | Kaiju + Fearful, ArticulatedTail
Hamm | | Ball + Heavy
Mr. Potato Head | | Biped + Swarm
Slinky Dog | | Quadruped + Link, Pull
Little Green Men | toy story aliens | Biped + Swarm, Playful
Zurg | emperor zurg | Biped + Shoots, Explodes
Lotso | | Biped + Sneaky
Forky | | Biped + Playful
Duke Caboom | | Vehicle + Launch, Fast
Buzz Lightyear's laser | wrist laser | Gun + Beam
Sox | sox the cat | Quadruped + Playful
Lightyear ship | xl-15 | Vehicle + Flies, Thrust
Sulley | james p sullivan | Biped + Charge, Heavy
Mike Wazowski | | Ball + Bounce, Playful
Boo | | Biped + Playful
Randall | | Serpent + Sneaky
Bob Parr | mr. incredible | Biped + Blunt, Heavy
Elastigirl | | Whip + Constrict, Pull
Dash | | Biped + Fast
Violet's force field | violet parr | Dome + Barrier
Jack-Jack | | Blob + Chaotic, Fire
Frozone | | Ramp + Launch, Ice
Syndrome | | Gauntlet + Grab, Psychic
Omnidroid | | Arachnid + Grab, Shoots
Incredibile | | Vehicle + Fast, Piloted
Edna Mode | | Biped + Protective
Nemo | | Fish + Fast
Bruce the shark | finding nemo shark | Fish + Charge, Grab
Squirt | crush the turtle | Fish + Playful
Dory | | Fish + Playful
Hank the octopus | | Serpent + Constrict, Sneaky
Remy | ratatouille | Quadruped + Fast
EVE | eve robot | Bird + Flies, Beam
M-O | | Vehicle + Fast
Axiom | | Vehicle + Flies, Heavy
Carl's cane | | Staff + Blunt, Pull
Dug | | Quadruped + Playful
Kevin the bird | | Bird + Fast
Dug's squirrel | | Quadruped + Fast
Merida's bow | brave | Bow + Pierce
Mor'du | | Quadruped + Charge, Heavy
Joy | | Blob + Light, Playful
Anger | | Blob + Fire, Aggressive
Anxiety | | Blob + Fast, Chaotic
Sadness | | Blob + Water, Lazy
Bing Bong | | Biped + Playful
Rainbow rocket wagon | | Vehicle + Launch
Ember Lumen | ember | Biped + Fire
Wade Ripple | | Blob + Water
Luca | sea monster luca | Fish + Water, Playful
Mei's red panda | red panda turning red | Quadruped + Grow, Charge
Dante the dog | | Quadruped + Playful
Pepita | | Kaiju + Flies
Elio | | Biped + Flies
Glordon | | Serpent + Playful
Hoppers robot beaver | hoppers | Quadruped + Piloted
Mater | tow mater | Vehicle + Pull, Link
Cruz Ramirez | | Vehicle + Fast
Francesco Bernoulli | | Vehicle + Fast
Chick Hicks | | Vehicle + Charge
A Bug's Life Hopper | | Insect + Charge
Heimlich | | Insect + Lazy
Arlo | good dinosaur | Quadruped + Heavy
## Animation — Disney
Mickey Mouse | mickey | Biped + Playful
Sorcerer's Apprentice brooms | enchanted brooms | Staff + Swarm, Water
Stitch's plasma blaster | plasma blaster | Gun + Shoots, Explodes
Jumba | | Biped + Shoots
Experiment 627 | | Biped + Chaotic
Simba | | Quadruped + Charge
Mufasa | | Quadruped + Charge, Heavy
Scar | | Quadruped + Sneaky
Hyenas | | Quadruped + Swarm, Chaotic
Rafiki's staff | | Staff + Blunt
Timon and Pumbaa | pumbaa | Quadruped + Charge, Poison
Genie | | Blob + Flies, Grow, Psychic
Magic carpet | aladdin's carpet | Vehicle + Flies, Piloted
Genie lamp | magic lamp | Bomb + Grow
Jafar snake | jafar | Serpent + Constrict, Fire
Iago | | Bird + Playful
Abu | | Quadruped + Grab
Cave of Wonders | | Dome + Trap
Rajah | | Quadruped + Charge
Maleficent dragon | maleficent | Kaiju + Breath, Fire
Ursula | | Serpent + Constrict, Swarm
Flotsam and Jetsam | | Fish + Sneaky
Triton's trident | king triton | Spear + Beam, Water
Chernabog | | Kaiju + Shadow
Hades | | Biped + Fire
Pegasus hercules | | Quadruped + Flies, Piloted
Hydra hercules | | Serpent + Swarm, Grab
Mushu | | Serpent + Breath, Fire
Shan Yu | | Sword + Blade
Beast | the beast | Quadruped + Charge, Heavy
Lumiere | | Biped + Fire, Playful
Cogsworth | | Biped + Gears
Mrs. Potts | chip | Ball + Bounce
Tinker Bell | | Insect + Flies, Glide
Captain Hook's hook | captain hook | Gauntlet + Pierce, Pull
Tick-Tock the crocodile | tick tock croc | Kaiju + Grab
Rapunzel's frying pan | frying pan | Hammer + Blunt, Guard
Rapunzel's hair | | Whip + Pull, Link
Pascal | | Quadruped + Sneaky
Maximus | | Quadruped + Charge
Olaf's snow flurry | | Blob + Ice
Marshmallow | frozen snow monster | Biped + Heavy, Ice
Sven | | Quadruped + Charge, Piloted
Bruni | fire salamander | Quadruped + Fire
Earth giants | | Kaiju + Stomp, Earth
Nokk | the nokk | Quadruped + Water, Piloted
Hei Hei | heihei | Bird + Chaotic
Pua | | Quadruped + Playful
Tamatoa | | Arachnid + Grab
Kakamora | | Biped + Swarm
Te Fiti | | Kaiju + Earth, Protective
Mirabel | encanto | Biped + Playful
Luisa | | Biped + Heavy
Bruno | | Biped + Sneaky
Namaari's dragon | sisu | Serpent + Flies, Water
Druun | | Blob + Swarm, Shadow
Judy Hopps | | Quadruped + Fast
Nick Wilde | | Quadruped + Sneaky
Gary De'Snake | | Serpent + Constrict
Flash the sloth | | Quadruped + Lazy
Baymax fist | rocket fist | Gauntlet + Thrust, Blunt
Microbots | | Blob + Swarm, Psychic
Ralph | wreck-it ralph | Biped + Blunt, Heavy
Cy-Bugs | | Insect + Swarm
Kronk | | Biped + Playful
Lava monster | | Kaiju + Fire
Mirror of the Evil Queen | magic mirror | Wall + Portal
Poison apple | | Ball + Poison
Cruella's car | | Vehicle + Charge
Dalmatians | 101 dalmatians | Quadruped + Swarm, Playful
Winnie the Pooh | pooh | Biped + Lazy
Tigger | | Quadruped + Bounce, Playful
Eeyore | | Quadruped + Lazy
Heffalumps | | Quadruped + Swarm
Kermit | | Biped + Bounce
Animal | | Biped + Chaotic
Miss Piggy | | Biped + Charge
## Animation — DreamWorks / Illumination / others
Po | dragon warrior | Biped + Bounce, Blunt
Tai Lung | | Biped + Fast, Charge
Tigress | | Biped + Fast, Blunt
Kai | | Whip + Link, Pull
Master Shifu | | Biped + Fast
Oogway's staff | | Staff + Guard
Kung Fu Panda wuxi finger hold | | Gauntlet + Trap
Shrek's swamp | | Dome + Poison
Fiona | | Biped + Blunt
Lord Farquaad | | Biped + Lazy
Mongo | giant gingerbread man | Kaiju + Stomp
Fairy Godmother's wand | | Staff + Beam, Psychic
Rumpelstiltskin | | Biped + Sneaky
Stormfly | | Quadruped + Flies, Shoots
Meatlug | | Quadruped + Flies, Heavy
Hookfang | | Quadruped + Flies, Fire
Barf and Belch | zippleback | Serpent + Flies, Breath
Red Death | | Kaiju + Breath, Fire
Bewilderbeast | | Kaiju + Breath, Ice
Light Fury | | Quadruped + Flies, Sneaky
Grimmel | | Bow + Poison
Megamind | | Gun + Beam
De-gun | dehydration gun | Gun + Shrink
Minion (Megamind) | brain bot | Bird + Swarm, Flies
Alex the lion | madagascar | Quadruped + Charge
Penguins of Madagascar | skipper | Biped + Swarm, Sneaky
King Julien | | Biped + Playful
Mr. Wolf | the bad guys | Quadruped + Sneaky
Mr. Snake | | Serpent + Constrict
Mr. Shark | | Fish + Charge
Puss in Boots' sword | | Sword + Blade, Fast
Death wolf | the wolf puss in boots | Quadruped + Sneaky, Shadow
Roz | the wild robot | Biped + Protective, Grab
Brightbill | | Bird + Flies
Trolls | poppy | Biped + Playful, Swarm
Bergens | | Biped + Heavy
Spirit | spirit stallion | Quadruped + Fast
Boss Baby | | Biped + Playful
Captain Underpants | | Biped + Flies, Playful
Dog Man | | Biped + Charge
Gru | | Gun + Shoots
Dr. Nefario | | Gun + Grow
Vector | | Gun + Shoots, Sticky
Squid launcher | vector's squid launcher | Gun + Sticky, Trap
Piranha gun | | Gun + Swarm
Lucy Wilde's lipstick taser | | Gun + Electric
Evil minions | purple minions | Biped + Swarm, Chaotic
El Macho | | Biped + Charge, Heavy
Mega Minions | | Biped + Flies, Beam
Kyle the dog | gru's dog | Quadruped + Chaotic
Unicorn fluffy | it's so fluffy | Quadruped + Bounce
Scarlet Overkill | | Staff + Launch
Rocket shoes | | Jetpack + Thrust
Max the dog | secret life of pets | Quadruped + Playful
Snowball the rabbit | | Quadruped + Chaotic
Sing's Johnny | | Biped + Playful
Mario's Cappy | | Disc + Returning
The Lorax | lorax | Biped + Protective
Horton | | Quadruped + Heavy
Grinch | the grinch | Biped + Sneaky, Grab
Grinch's sleigh | | Vehicle + Piloted
Max the reindeer dog | | Quadruped + Pull
Totoro | | Quadruped + Lazy, Bounce | @Totoro
Catbus | | Quadruped + Fast, Piloted | @Catbus
No-Face | kaonashi | Blob + Grab, Swarm | @No-Face
Howl's Moving Castle | moving castle | Vehicle + Walks, Heavy | @Howl's Moving Castle (film)
Calcifer | | Blob + Fire | @Calcifer
Laputa robot | robot soldier | Biped + Beam | @Castle in the Sky
Ohmu | | Insect + Charge, Heavy
Ponyo | | Fish + Water, Playful
Soot sprites | susuwatari | Blob + Swarm | @Susuwatari
Haku dragon | | Serpent + Flies
Forest spirit | shishigami | Quadruped + Grow
Kodama | | Blob + Swarm
Kiki's broom | | Vehicle + Flies, Piloted
Porco Rosso plane | | Vehicle + Flies, Shoots
Laika's Coraline other mother | beldam | Arachnid + Sticky, Trap
Iron Giant's ray | | Gun + Beam
Hogarth | | Biped + Playful
Wallace and Gromit's techno trousers | techno trousers | Vehicle + Walks, Sticky
Were-Rabbit | | Quadruped + Charge
Feathers McGraw | | Bird + Sneaky
Shaun the Sheep | | Quadruped + Playful
Chicken Run chickens | | Bird + Swarm
Mr. Tweedy's pie machine | | Cage + Trap, Grab
## Classic cartoons & TV animation
Bugs Bunny | | Quadruped + Playful, Fast | @Bugs Bunny
Daffy Duck | | Bird + Chaotic | @Daffy Duck
Wile E. Coyote | wile e coyote | Quadruped + Chaotic
ACME catapult | | Ramp + Launch
ACME giant magnet | giant magnet | Pillar + Attract
ACME earthquake pills | | Bomb + Stomp, Earth
ACME jet-propelled unicycle | | Vehicle + Thrust
ACME bat-man outfit | | Wings + Glide
ACME spring shoes | | Trampoline + Bounce
ACME piano | falling piano | Anvil + DropFromSky, Heavy
Yosemite Sam | | Gun + Shoots, Aggressive
Elmer Fudd's shotgun | | Gun + Shoots
Foghorn Leghorn | | Bird + Charge
Speedy Gonzales | | Quadruped + Fast
Sylvester | | Quadruped + Sneaky
Tweety | | Bird + Flies, Playful
Marvin the Martian | | Gun + Beam
Pepe Le Pew | | Quadruped + Poison
Gossamer | | Biped + Heavy
Tom and Jerry | tom cat | Quadruped + Chaotic
Jerry's mallet | | Hammer + Blunt
Spike the bulldog | | Quadruped + Charge
Scooby-Doo | scooby doo | Quadruped + Fearful
Mystery Machine | | Vehicle + Charge
Scooby Snacks | | Ball + Swarm
Fred Flintstone's car | flintmobile | Vehicle + Charge
Dino | flintstones dino | Quadruped + Playful
Jetsons car | | Vehicle + Flies
Rosie the robot | | Biped + Protective
Space Ghost | | Gauntlet + Beam
Johnny Bravo | | Biped + Blunt
Powerpuff Girls | blossom; bubbles; buttercup | Biped + Swarm, Flies
Mojo Jojo | | Biped + Piloted
Dexter's robot | dexter's lab robot | Kaiju + Piloted
Samurai Jack's sword | samurai jack | Sword + Blade
Aku | | Kaiju + Shadow, Fire
Courage the Cowardly Dog | courage | Quadruped + Fearful
Ben 10 Omnitrix | omnitrix | Gauntlet + Grow
Heatblast | | Biped + Fire
Four Arms | | Biped + Blunt, Grab
Diamondhead | | Biped + Blade, Earth
XLR8 | | Biped + Fast
Way Big | | Kaiju + Beam
He-Man | | Sword + Blade, Heavy
Beast Man | | Biped + Charge
Man-At-Arms | | Hammer + Blunt
Wind Raider | | Vehicle + Flies, Piloted
She-Ra's sword | sword of protection | Sword + Blade, Light
Swift Wind | | Quadruped + Flies, Piloted
ThunderCats Sword of Omens | sword of omens | Sword + Blade, Beam
Mumm-Ra | | Biped + Shadow
Voltron lions | | Quadruped + Flies, Piloted
Gargoyles Goliath | goliath gargoyles | Wings + Glide, Charge
Gummi Bears | | Biped + Bounce
Inspector Gadget | gadget | Gauntlet + Pull, Grab
Go Go Gadget copter | gadget copter | Wings + SlowFall
Dr. Claw's cat | mad cat | Quadruped + Sneaky
Darkwing Duck gas gun | darkwing duck | Gun + Poison
Launchpad's plane | | Vehicle + Flies, Chaotic
Scrooge's money bin | | Cage + Heavy
Gizmoduck | | Vehicle + Piloted, Shoots
Kim Possible grappling hairdryer | | Gun + Pull
Phineas and Ferb rollercoaster | | Ramp + Launch
Perry the Platypus | agent p | Quadruped + Sneaky
Doofenshmirtz inator | inator | Gun + Beam, Shrink
Gravity Falls Bill Cipher | bill cipher | Blob + Flies, Chaotic
Gideon's amulet | | Gauntlet + Grab, Psychic
Gobblewonker | | Serpent + Water
Mabel's grappling hook | grappling hook | Gun + Pull
Aang's glider | aang's staff; air glider | Staff + Glide, Wind
Appa | | Quadruped + Flies, Piloted
Momo | | Bird + Playful
Katara's water whip | water whip | Whip + Pull, Water
Toph's earthbending | earth bending | Boulder + Earth, Heavy
Zuko's fire | firebending | Staff + Breath, Fire
Sokka's boomerang | | Disc + Returning
Space sword | sokka's sword | Sword + Blade
Ozai's lightning | | Staff + Beam, Electric
Koh the face stealer | | Serpent + Sneaky
Kyoshi fans | | Disc + Wind, Repel
Korra's mech | hummingbird suit | Biped + Piloted
Naga the polar bear dog | naga | Quadruped + Charge, Piloted
Plankton | | Blob + Sneaky, Chaotic
Chum Bucket robot | | Biped + Piloted
Patrick Star | | Blob + Lazy, Heavy
Squidward's clarinet | squidward | Staff + Repel
Sandy Cheeks | | Biped + Blunt
Mr. Krabs | | Arachnid + Grab
Gary the snail | gary | Blob + Sticky
Mrs. Puff | | Fish + Grow
Flying Dutchman | | Biped + Flies, Shadow
Man Ray | | Biped + Beam
Invisible Boatmobile | boatmobile | Vehicle + Charge, Sneaky
Bubble wand | bubble blowing | Staff + Bounce, Water
Reef blower | | Staff + Pull
Homer Simpson | homer | Biped + Heavy, Lazy
Bart's slingshot | bart simpson | Bow + Shoots
Bart's skateboard | | Vehicle + Fast
Itchy and Scratchy | | Quadruped + Chaotic
Mr. Burns' hounds | | Quadruped + Swarm
Kang and Kodos | | Blob + Beam
Duff blimp | | Vehicle + Flies
Monorail | | Vehicle + Fast
Radioactive Man | | Biped + Atomic
Blinky the three-eyed fish | blinky | Fish + Atomic
Bender | | Biped + Blunt, Heavy
Planet Express ship | | Vehicle + Flies, Fast
Nibbler | | Quadruped + Grab
Hypnotoad | | Blob + Psychic
Zoidberg | | Arachnid + Grab
Morty | | Biped + Fearful
Rick's spaceship | space cruiser | Vehicle + Flies, Shoots
Mr. Poopybutthole | | Biped + Playful
Plumbus | | Staff + Blunt
Squanchy | | Quadruped + Chaotic
Unity | | Blob + Swarm, Psychic
Gearhead | | Biped + Gears
Birdperson | | Bird + Flies, Shoots
Snuffles | snowball dog | Quadruped + Piloted
Council of Ricks | | Biped + Swarm
Evil Morty | | Gun + Sneaky
Finn the Human | finn | Sword + Blade
Ice King | | Staff + Ice, Flies
Marceline's axe bass | marceline | Axe + Blade, Shadow
Princess Bubblegum | | Blob + Sticky
Lady Rainicorn | | Serpent + Flies
Gunter | | Bird + Chaotic
BMO | | Biped + Playful
The Lich | | Biped + Shadow
Lumpy Space Princess | lsp | Blob + Flies
Bingo | | Quadruped + Playful
Bandit Heeler | | Quadruped + Playful
Chilli Heeler | | Quadruped + Protective
Muffin | | Quadruped + Chaotic
## Transformers
Megatron | | Biped + Shoots, Heavy | @Megatron
Starscream | | Vehicle + Flies, Shoots | @Starscream
Soundwave | | Biped + Swarm, Repel | @Soundwave (Transformers)
Laserbeak | ravage | Bird + Flies, Shoots
Shockwave | | Biped + Beam | @Shockwave (Transformers)
Grimlock | | Kaiju + Breath, Fire | @Grimlock
Dinobots | | Kaiju + Swarm, Charge
Ironhide | | Vehicle + Shoots, Heavy
Ratchet | | Vehicle + Protective
Jazz | | Vehicle + Fast
Arcee | | Vehicle + Fast
Sideswipe | | Vehicle + Blade
Unicron | | Ball + Attract, DropFromSky
Sentinel Prime | | Biped + Guard, Heavy
Scourge | | Biped + Flies
Unicron's heralds | | Biped + Swarm
The Fallen | | Staff + Beam
Scorponok | | Arachnid + ArticulatedTail
Sweeps | | Vehicle + Swarm
Constructicons | | Vehicle + Swarm
Optimus Primal | | Biped + Blade
Rhinox | | Quadruped + Charge
Mirage | | Vehicle + Sneaky
Wheeljack | | Vehicle + Explodes
Energon | energon cube | Ball + Explodes, Electric
Allspark | | Ball + Grow, Electric
Matrix of Leadership | | Ball + Light
Predaking | | Kaiju + Breath, Flies
## Ghostbusters / Back to the Future / Doctor Who / Star Trek / sci-fi
Ecto-goggles | | Gauntlet + Sneaky
Ghost trap pedal | | Bomb + Trap
Zuul | terror dog | Quadruped + Charge, Shadow
Gozer | | Biped + Beam
Vigo | | Wall + Shadow
Mini-Pufts | mini puft | Biped + Swarm, Playful
Garraka | | Kaiju + Ice
Proton pack boson dart | boson dart | Gun + Explodes
Muncher | | Blob + Grab
Doc Brown | | Biped + Electric
Flux capacitor | | Bomb + Portal, Electric
Mr. Fusion | | Bomb + Atomic
Time train | jules verne train | Vehicle + Flies, Portal
Biff's Hoverboard | pit bull hoverboard | Vehicle + Fast
Clock tower | hill valley clock tower | Pillar + Electric
K-9 | k9 | Quadruped + Beam
Adipose | | Blob + Swarm, Playful
Ood | | Biped + Swarm, Psychic
Sontarans | | Biped + Swarm, Blunt
Silurians | | Biped + Shoots
Slitheen | | Biped + Explodes
The Master's laser screwdriver | laser screwdriver | Staff + Beam, Shrink
Tissue compression eliminator | | Gun + Shrink
Davros | | Vehicle + Beam
Great Intelligence snowmen | | Biped + Swarm, Ice
Vashta Nerada | | Blob + Swarm, Shadow
Racnoss | | Arachnid + Grab
Toclafane | | Ball + Swarm, Blade
Captain Picard | picard | Biped + Protective
Spock | | Gauntlet + Grab
Klingon Bird of Prey | bird of prey | Vehicle + Flies, Sneaky
Romulan Warbird | | Vehicle + Flies, Beam
Photon torpedo | | Rocket + Explodes, Homing
Gorn | | Biped + Blunt, Heavy
Horta | | Blob + Earth
Borg drone | | Biped + Swarm
Q | | Blob + Chaotic, Psychic
Shuttlecraft | | Vehicle + Flies
Transporter | beam me | Platform + Portal
Holodeck | | Cage + Trap
Lirpa | | Staff + Blade, Blunt
Serenity | firefly ship | Vehicle + Flies
Reavers | | Biped + Swarm, Aggressive
Stargate | | Platform + Portal | @Stargate (device)
Goa'uld staff weapon | staff weapon | Staff + Shoots
Replicators | | Arachnid + Swarm
Battlestar Galactica | galactica | Vehicle + Flies, Shoots
Cylon raider | | Vehicle + Flies, Swarm
Cylon centurion | | Biped + Swarm, Shoots
Viper | colonial viper | Vehicle + Fast, Flies
The Expanse Rocinante | rocinante | Vehicle + Flies, Shoots
Protomolecule | | Blob + Swarm, Sticky
Normandy | mass effect normandy | Vehicle + Flies
Babylon 5 station | | Pillar + Barrier
## Horror & monster icons
Freddy Krueger's glove | freddy's claw; freddy krueger | Gauntlet + Blade, Sneaky | @Freddy Krueger
Jason's machete | jason voorhees | Sword + Blade | @Jason Voorhees
Leatherface's chainsaw | leatherface; chainsaw | Sword + Blade, Spin | @Leatherface
Michael Myers | | Biped + Sneaky, Heavy | @Michael Myers (Halloween)
Chucky | good guy doll | Biped + Sneaky, Fast | @Chucky
Annabelle | | Biped + Sneaky | @Annabelle (doll)
M3GAN | megan | Biped + Fast, Sneaky | @M3GAN
Pennywise | it clown | Biped + Sneaky, Swarm | @Pennywise
Pennywise balloon | red balloon | Ball + SlowFall, Sneaky
Ghostface | | Sword + Blade, Sneaky | @Ghostface (identity)
Pinhead | | Whip + Link, Pull | @Pinhead (Hellraiser)
Hellraiser chains | cenobite chains | Whip + Link, Pull, Pierce
Lament Configuration | puzzle box | Cage + Trap, Portal
Pyramid Head | | Sword + Heavy
Candyman's hook | | Gauntlet + Pierce
Slenderman | slender man | Biped + Sneaky
Art the Clown | | Biped + Chaotic
Terrifier | | Biped + Chaotic
Five Nights at Freddy's animatronics | freddy fazbear | Biped + Swarm, Sneaky
Foxy | | Biped + Charge, Pierce
Chica | | Biped + Sneaky
Bonnie | | Biped + Sneaky
Springtrap | | Biped + Sneaky
Dracula | count dracula | Bird + Flies, Swarm, Shadow | @Count Dracula
Frankenstein's monster | frankenstein | Biped + Heavy, Electric | @Frankenstein's monster
The Mummy | imhotep | Biped + Swarm, Earth
Scarab beetles | mummy scarabs | Insect + Swarm
Wolf Man | werewolf | Quadruped + Charge, Aggressive
Creature from the Black Lagoon | gill-man | Biped + Grab, Water | @Gill-man
Invisible Man | | Biped + Sneaky
Headless Horseman | | Quadruped + Piloted, Fire
Jack Skellington | | Biped + Sneaky
Oogie Boogie | | Blob + Swarm
Beetlejuice | | Serpent + Chaotic
Sandworm (Beetlejuice) | saturn sandworm | Serpent + Grab, Charge
Gremlin Stripe | stripe | Biped + Chaotic
Critters | krites | Ball + Swarm, Aggressive
Killer Klowns | | Biped + Swarm, Sticky
The Thing (1982) | the thing monster | Blob + Grab, Constrict
Tall Man's sphere | phantasm sphere | Ball + Homing, Blade
Evil Dead chainsaw arm | boomstick | Gauntlet + Blade, Spin
Deadites | | Biped + Swarm, Chaotic
Zombies | zombie horde | Biped + Swarm, Aggressive
Sinners vampires | | Bird + Swarm, Shadow
Nosferatu | count orlok | Bird + Swarm, Shadow
Weapons Aunt Gladys | | Biped + Chaotic
The Conjuring demon | valak | Biped + Sneaky
Smile entity | | Biped + Sneaky
Barbarian's mother | | Biped + Grab
A Quiet Place creature | death angel | Biped + Fast, Sneaky
Bird Box | | Blob + Sneaky
Nope Jean Jacket | jean jacket | Blob + Flies, Grab
Cujo | | Quadruped + Charge, Aggressive
Christine | killer car | Vehicle + Charge
The Birds | | Bird + Swarm, Aggressive
Piranha | piranhas | Fish + Swarm
Anaconda | | Serpent + Constrict
Lake Placid croc | | Kaiju + Grab
Arachnophobia spiders | | Arachnid + Swarm
Eight Legged Freaks | | Arachnid + Swarm, Grow
Leprechaun | | Biped + Playful
Tremors Graboid | | Serpent + Earth
Pumpkinhead | | Biped + Charge
Krampus | | Biped + Grab
## Live action — action, sci-fi, fantasy franchises
Neo | the one matrix | Biped + Fast, Guard
Agent Smith | | Biped + Swarm
Sentinels (Matrix) | squiddy | Serpent + Flies, Swarm, Grab
APU mech | | Biped + Piloted, Shoots
Nebuchadnezzar | | Vehicle + Flies
RoboCop | | Gun + Shoots, Heavy | @RoboCop
ED-209 | | Biped + Shoots, Heavy | @ED-209
T-800 endoskeleton | | Biped + Shoots
Hunter-Killer | | Vehicle + Flies, Shoots
Skynet | | Ball + Swarm
Judge Dredd's Lawgiver | lawgiver | Gun + Shoots, Homing
Mad Max Interceptor | pursuit special | Vehicle + Fast, Charge
Doof Warrior | flame guitar | Staff + Breath, Fire
Immortan Joe | | Vehicle + Charge
Gigahorse | | Vehicle + Heavy, Charge
Fast and Furious Charger | dom's charger | Vehicle + Charge, Fast
Nitrous boost | nos | Jetpack + Thrust, Fire
Aston Martin DB5 | bond car | Vehicle + Shoots, Launch
Oddjob's hat | | Disc + Blade, Returning
Jaws (Bond) | jaws steel teeth | Biped + Grab
Golden Gun | | Gun + Pierce
Q's jetpack | bell rocket belt | Jetpack + Thrust
Little Nellie | | Vehicle + Flies, Shoots
Moonraker laser | | Gun + Beam
Mission Impossible mask | | Biped + Sneaky
Top Gun F-14 | tomcat; maverick's jet | Vehicle + Flies, Shoots, Fast
Darkstar | | Vehicle + Fast, Flies
Twisters tornado | tornado | Blob + Spin, Wind, Attract
Sharknado chainsaw | | Sword + Spin, Blade
Kill Bill katana | hattori hanzo sword | Sword + Blade
John Wick's pencil | john wick | Spear + Pierce, Fast
Gladiator Maximus | | Sword + Blade
Excalibur | sword in the stone | Sword + Blade, Light
Mjolnir (The Mighty Thor) | | Hammer + Returning
Highlander katana | | Sword + Blade
Conan's sword | atlantean sword | Sword + Blade, Heavy
Robin Hood's bow | | Bow + Pierce
Zorro's rapier | zorro | Sword + Pierce, Fast
Pirates of the Caribbean Kraken | davy jones' kraken | Serpent + Constrict, Water
Black Pearl | | Vehicle + Water, Shoots
Flying Dutchman ship | | Vehicle + Water, Shadow
Davy Jones | | Biped + Grab, Water
Jack Sparrow's compass | | Disc + Homing
Jumanji rhino stampede | stampede | Quadruped + Swarm, Charge
Jumanji monkeys | | Quadruped + Swarm, Chaotic
Men in Black neuralyzer | neuralyzer | Gauntlet + Repel, Light
Noisy Cricket | | Gun + Repel, Explodes
J2 series | | Gun + Shoots
Frank the pug | | Quadruped + Playful
Worm guys | | Serpent + Swarm
Edgar the bug | | Arachnid + Grab
Inception spinning top | totem | Ball + Spin
Tenet turnstile | | Platform + Portal
Interstellar TARS | tars | Biped + Walks
Endurance spacecraft | | Vehicle + Spin, Flies
Gargantua | black hole | Ball + Attract, Explodes
Oppenheimer's bomb | trinity | Bomb + Explodes, Atomic
Hail Mary ship | project hail mary ship | Vehicle + Flies
Xenonite | | Wall + Barrier
Disclosure Day UFO | | Vehicle + Flies, Beam
Close Encounters mothership | | Vehicle + Flies, Beam
E.T. | extra-terrestrial | Biped + Psychic, Light
E.T.'s bike | flying bmx | Vehicle + Flies, Piloted
Gremlins Gizmo | gizmo | Biped + Playful
The Goonies' pirate ship | inferno | Vehicle + Water
Ghost rider penance stare | | Gun + Beam, Fire
Dune crysknife | crysknife | Sword + Blade, Pierce
Dune shield | holtzman shield | Dome + Barrier, Repel
Gom jabbar | | Spear + Pierce, Poison
Harkonnen | beast rabban | Biped + Heavy
Baron Harkonnen | | Biped + Flies
Paul Atreides | muad'dib | Biped + Sneaky
Weirding module | the voice | Gun + Repel
Sardaukar | | Biped + Swarm, Blade
Spice harvester | harvester | Vehicle + Heavy, Treads
Carryall | | Vehicle + Flies, Grab
Na'vi bow | | Bow + Pierce
Neytiri | | Bow + Fast
Direhorse | | Quadruped + Charge, Piloted
Thanator | | Quadruped + Charge, Shadow
Viperwolf | viperwolves | Quadruped + Swarm
Hammerhead titanothere | | Quadruped + Charge, Heavy
Skimwing | | Fish + Flies, Piloted
Ilu | | Fish + Piloted, Water
Crabsuit | | Arachnid + Piloted, Grab
Varang | ash people | Biped + Fire
Sea Dragon ship | | Vehicle + Water, Shoots
Scorpion gunship | | Vehicle + Flies, Shoots
Samson helicopter | | Vehicle + Flies, Rotor
Game of Thrones Drogon | drogon | Kaiju + Flies, Breath, Fire
Rhaegal | | Kaiju + Flies, Breath
Viserion | ice dragon | Kaiju + Flies, Breath, Ice
Vhagar | | Kaiju + Flies, Breath, Heavy
Caraxes | the blood wyrm | Serpent + Flies, Breath
Syrax | | Kaiju + Flies, Breath
Sunfyre | | Kaiju + Flies, Breath
Meleys | | Kaiju + Flies, Fast
Vermithor | | Kaiju + Flies, Heavy
Dreamfyre | | Kaiju + Flies, Breath
White Walkers | white walker | Biped + Ice, Swarm
Night King's spear | night king | Spear + Pierce, Ice
Wights | | Biped + Swarm, Ice
Ice wight bear | | Quadruped + Charge, Ice
Ghost the direwolf | direwolf | Quadruped + Charge
Longclaw | valyrian steel sword | Sword + Blade
Needle | arya's sword | Sword + Pierce, Fast
Oathkeeper | | Sword + Blade
Dragonglass | | Spear + Pierce
Wildfire | | Bomb + Explodes, Fire
Scorpion ballista | | Cannon + Pierce
The Wall | wall of westeros | Wall + Barrier, Ice
Iron Throne | | Pillar + Blade
The Mountain | gregor clegane | Biped + Heavy
Hodor | | Biped + Protective, Heavy
Giant wun wun | wun wun | Kaiju + Grab
Witcher's silver sword | silver sword | Sword + Blade, Light
Geralt | | Sword + Blade
Aard sign | aard | Gauntlet + Repel
Igni | | Gauntlet + Breath, Fire
Yrden | | Cage + Trap
Roach | | Quadruped + Piloted
Leshen | | Biped + Swarm, Earth
Striga | | Biped + Charge
Wheel of Time Trolloc | trollocs | Biped + Swarm
Shadow and Bone volcra | volcra | Bird + Swarm, Shadow
The Last of Us clicker | clicker | Biped + Swarm, Sneaky
Bloater | | Biped + Heavy, Poison
Fallout power armor | power armor | Biped + Piloted, Heavy
Fat Man launcher | fat man | Cannon + Explodes, Atomic
Deathclaw | | Biped + Charge, Blade
The Mandalorian's Razor Crest | | Vehicle + Flies
Andor TIE Avenger | tie avenger | Vehicle + Fast, Flies
The Rings of Power Balrog | | Kaiju + Fire
Percy Jackson's Riptide | riptide | Sword + Blade, Returning
Minotaur (Percy Jackson) | | Biped + Charge
Chimera | | Quadruped + Breath, Fire
Narnia Aslan | aslan | Quadruped + Charge, Light
White Witch's wand | | Staff + Ice, Trap
Neverending Story Falkor | falkor; luck dragon | Serpent + Flies, Piloted
Gmork | | Quadruped + Sneaky
Rockbiter | | Biped + Earth, Heavy
Labyrinth Ludo | ludo | Biped + Earth
Dark Crystal Skeksis | skeksis | Bird + Sneaky
Willow's wand | | Staff + Shrink
Eborsisk | | Serpent + Breath, Fire
Ghostbusters Firehouse | | Wall + Barrier
Indiana Jones idol | golden idol | Ball + Trap
Ark of the Covenant | the ark | Pillar + Beam, Repel
Holy Grail | | Ball + Light
Crystal skull | | Ball + Psychic
Dial of Destiny | antikythera | Disc + Portal
Nazi flying wing | | Vehicle + Flies
Temple of Doom mine cart | mine cart | Vehicle + Charge, Fast
Sankara stones | | Boulder + Fire
Snakes (Indiana Jones) | why did it have to be snakes | Serpent + Swarm, Poison
## Power Rangers / tokusatsu / misc TV
Power Rangers | mighty morphin | Biped + Swarm
Power Morpher | morpher | Gauntlet + Grow
Dragonzord | | Kaiju + Shoots
Tyrannosaurus Zord | | Kaiju + Stomp
Rita Repulsa's wand | rita's staff | Staff + Grow
Goldar | | Wings + Blade
Putty Patrollers | putties | Biped + Swarm
Lord Zedd | | Staff + Beam
Ultraman | ultraman specium ray | Kaiju + Beam, Flies
Kamen Rider | | Vehicle + Fast, Piloted
Thunderbirds Thunderbird 2 | thunderbird 2 | Vehicle + Flies, Heavy
International Rescue | | Vehicle + Flies
Knight Rider KITT | kitt | Vehicle + Fast, Launch
Airwolf | | Vehicle + Flies, Shoots
Street Hawk | | Vehicle + Fast
A-Team van | | Vehicle + Charge
MacGyver's knife | | Sword + Blade
Buffy's stake | mr pointy | Spear + Pierce
Xena's chakram | chakram | Disc + Returning, Ricochet
Hercules' club | | Hammer + Blunt
Wonder Twins | | Blob + Water
Supernatural Impala | | Vehicle + Charge
Colt | supernatural colt | Gun + Pierce
Sabrina's cat Salem | salem | Quadruped + Playful
Mighty Ducks puck | | Disc + Ricochet
Space Jam Monstars | monstars | Biped + Swarm, Grow
Tune Squad | | Biped + Swarm, Playful
Who Framed Roger Rabbit dip | the dip | Blob + Poison
Benny the cab | | Vehicle + Chaotic
Toon hammer | giant toon mallet | Hammer + Blunt, Bounce
Muppets Gonzo's cannon | human cannonball | Cannon + Launch
Paddington's marmalade | | Ball + Sticky
Mr. Bean's Mini | mini cooper | Vehicle + Charge
Teddy (Mr. Bean) | | Biped + Playful
Wallace's rocket | | Rocket + Thrust
Thunderbirds Thunderbird 1 | | Rocket + Fast
Sonic the Hedgehog (film) | | Ball + Fast, Spin
Knuckles (film) | | Gauntlet + Blunt
Shadow (film) | | Ball + Fast, Shadow
Dr. Robotnik's badniks | badniks | Arachnid + Swarm
"""
