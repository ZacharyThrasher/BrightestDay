"""Generate assets/icons/anime.ron (+ preview JSON for render.py).

    python tools/icons/anime/gen.py          # writes assets/icons/anime.ron
    python tools/icons/anime/gen_names.py    # writes assets/icons/anime_names.txt from names*.txt
    python tools/icons/anime/render.py <preview.json> out.png [id,id,...]   # contact sheet (mirrors bd_shapes voxelize)
BD_ROOT overrides the repo root; PREVIEW_DIR the preview JSON folder (default: system temp).
"""
import json, sys, zlib, os, tempfile
HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
ROOT = os.environ.get("BD_ROOT") or os.path.abspath(os.path.join(HERE, "..", "..", ".."))
import held as H
import creatures as C
from shapes import katana

OUT = os.path.join(ROOT, "assets", "icons", "anime.ron")
PREVIEW = os.path.join(os.environ.get("PREVIEW_DIR") or tempfile.gettempdir(), "anime_icons_preview.json")

E = []


def icon(names, display, form, mech, scale, mass, *, element='Light', count=1, accent=None, features=(),
         temperament=None, intensity=None, shape=None, source='', payload=None, id=None):
    E.append(dict(names=names, display=display, form=form, mech=list(mech), scale=scale, mass=mass, element=element,
                  count=count, accent=accent, features=list(features), temperament=temperament, intensity=intensity,
                  shape=shape, source=source, payload=payload, id=id or names[0].replace(' ', '_').replace("'", '').replace('-', '_')))


RED = (0.9, 0.18, 0.15); ORANGE = (1.0, 0.55, 0.1); YELLOW = (1.0, 0.85, 0.2); GOLD = (0.95, 0.78, 0.3)
BLUE = (0.35, 0.6, 1.0); CYAN = (0.55, 0.9, 1.0); PURPLE = (0.65, 0.3, 1.0); PINK = (1.0, 0.55, 0.75)
WHITE = (0.95, 0.95, 0.97); BLACK = (0.08, 0.08, 0.1); GREEN = (0.35, 0.9, 0.35); ICE = (0.7, 0.9, 1.0)

# ======================================================================================= Dragon Ball
DB = "Dragon Ball"
icon(["kamehameha", "kamehame ha", "kamehameha wave", "kame hame ha", "kamehamea", "kamehameah", "goku's kamehameha"],
     "Kamehameha", "Staff", ["Beam", "Heavy"], 1.0, 5, accent=CYAN, shape=H.kamehameha(), source=DB)
icon(["spirit bomb", "genki dama", "genkidama", "spirit ball", "goku's spirit bomb"],
     "Spirit Bomb", "Ball", ["DropFromSky", "Explodes", "Heavy"], 7.0, 500, accent=CYAN, shape=H.spirit_bomb(), source=DB)
icon(["flying nimbus", "nimbus", "kinto'un", "kintoun", "kinton", "nimbus cloud", "goku's cloud", "flying cloud"],
     "Flying Nimbus", "Vehicle", ["Flies", "Piloted", "Fast"], 2.4, 20, accent=YELLOW, shape=H.nimbus(), source=DB)
icon(["power pole", "nyoibo", "nyoi-bo", "nyoi bo", "power staff", "goku's staff", "extending staff", "magic staff goku"],
     "Power Pole", "Spear", ["Blunt", "Fast"], 4.0, 8, accent=RED, shape=H.power_pole(), source=DB)
icon(["dragon balls", "dragon ball", "dragonballs", "seven dragon balls", "4 star dragon ball", "four star dragon ball"],
     "Dragon Balls", "Ball", ["Ricochet", "Bounce"], 0.6, 2, count=7, accent=ORANGE, shape=H.dragon_ball(), source=DB)
icon(["shenron", "shenlong", "shen long", "eternal dragon", "shenron dragon", "porunga"],
     "Shenron", "Serpent", ["Flies", "Constrict", "ArticulatedTail"], 4.0, 2000, accent=GREEN, shape=C.shenron(),
     temperament="Calm", source=DB)
icon(["destructo disc", "destructo disk", "kienzan", "krillin's disc", "krillin disc", "energy disc"],
     "Destructo Disc", "Disc", ["Blade", "Spin", "Pierce"], 1.8, 3, accent=YELLOW, shape=H.destructo_disc(), source=DB)
icon(["special beam cannon", "makankosappo", "makankosapo", "piccolo's beam", "piccolo beam", "light demon beam"],
     "Special Beam Cannon", "Staff", ["Beam", "Pierce", "Spin"], 2.5, 3, accent=YELLOW, shape=H.special_beam_cannon(), source=DB)
icon(["majin buu", "buu", "fat buu", "majin boo", "boo", "majinbuu"],
     "Majin Buu", "Blob", ["Bounce", "Shrink", "Grab"], 2.2, 150, accent=PINK, shape=C.majin_buu(),
     temperament="Playful", intensity=0.8, source=DB)
icon(["great ape oozaru", "great ape", "oozaru", "ozaru", "golden great ape", "saiyan great ape", "great ape goku"],
     "Great Ape Oozaru", "Kaiju", ["Stomp", "Breath", "Heavy", "ArticulatedTail"], 14.0, 30000, accent=(0.78, 0.58, 0.38), shape=C.great_ape(),
     temperament="Aggressive", intensity=0.9, source=DB)
icon(["instant transmission", "shunkan ido", "instant teleport", "goku's teleport"],
     "Instant Transmission", "Wall", ["Portal"], 2.5, 5, accent=CYAN, source=DB)
icon(["hakai", "beerus' hakai", "beerus hakai", "destruction orb", "god of destruction"],
     "Hakai", "Ball", ["Homing", "Explodes"], 0.8, 2, element="Shadow", accent=PURPLE, source=DB)
icon(["death ball", "frieza's death ball", "frieza death ball", "supernova"],
     "Death Ball", "Ball", ["DropFromSky", "Explodes", "Heavy"], 5.0, 300, element="Fire", accent=RED, source=DB)
icon(["final flash", "vegeta's final flash", "vegeta final flash"],
     "Final Flash", "Staff", ["Beam", "Heavy"], 1.2, 5, element="Electric", accent=YELLOW, source=DB)
icon(["galick gun", "galic gun", "garlic gun", "gyarikku ho"],
     "Galick Gun", "Staff", ["Beam"], 1.0, 4, element="Psychic", accent=PURPLE, source=DB)
icon(["big bang attack", "big bang", "vegeta's big bang"],
     "Big Bang Attack", "Ball", ["Explodes", "Fast"], 1.2, 5, accent=CYAN, source=DB)

# ======================================================================================= Naruto
NA = "Naruto"
icon(["rasengan", "resengan", "rasengun", "rasangan", "naruto's rasengan", "spiralling sphere", "spiraling sphere", "odama rasengan"],
     "Rasengan", "Ball", ["Spin", "Explodes"], 0.9, 3, element="Wind", accent=CYAN, shape=H.rasengan(), source=NA)
icon(["rasenshuriken", "rasen shuriken", "wind style rasenshuriken", "wind release rasenshuriken", "fuuton rasenshuriken"],
     "Rasenshuriken", "Disc", ["Blade", "Spin", "Explodes"], 2.8, 5, element="Wind", accent=CYAN, shape=H.rasenshuriken(), source=NA)
icon(["chidori", "raikiri", "lightning blade", "one thousand birds", "sasuke's chidori", "kakashi's raikiri"],
     "Chidori", "Gauntlet", ["Pierce", "Fast", "Thrust"], 1.0, 4, element="Electric", accent=CYAN, shape=H.chidori(), source=NA)
icon(["kurama", "nine tails", "nine-tails", "nine tailed fox", "nine-tailed fox", "kyuubi", "kyubi", "kurama fox"],
     "Kurama", "Kaiju", ["Stomp", "Breath", "ArticulatedTail"], 18.0, 40000, element="Fire", accent=ORANGE, shape=C.kurama(),
     temperament="Aggressive", intensity=0.7, source=NA)
icon(["susanoo", "susano'o", "susanoo sasuke", "perfect susanoo", "itachi's susanoo", "susano"],
     "Susanoo", "Kaiju", ["Piloted", "Barrier", "Blade"], 12.0, 20000, element="Shadow", accent=PURPLE, shape=C.susanoo(), source=NA)
icon(["shadow clone jutsu", "shadow clones", "shadow clone", "kage bunshin", "kage bunshin no jutsu", "multi shadow clone jutsu", "naruto clones"],
     "Shadow Clone Jutsu", "Biped", ["Swarm", "Blunt"], 1.6, 50, count=6, accent=ORANGE, shape=C.ninja(),
     temperament="Aggressive", intensity=0.7, source=NA)
icon(["kunai", "kunais", "ninja kunai", "kunai knife"],
     "Kunai", "Spear", ["Pierce", "Fast"], 0.5, 0.5, count=3, shape=H.kunai(), source=NA)
icon(["flying thunder god kunai", "flying raijin", "hiraishin", "hiraishin kunai", "minato's kunai", "three pronged kunai", "flying thunder god"],
     "Flying Thunder God Kunai", "Spear", ["Pierce", "Portal", "Fast"], 0.6, 0.6, accent=YELLOW, shape=H.kunai(True), source=NA)
icon(["naruto shuriken", "shuriken", "shurikens", "ninja star", "ninja stars", "throwing star", "throwing stars", "shuriken jutsu"],
     "Naruto Shuriken", "Disc", ["Blade", "Spin"], 0.35, 0.2, count=3, shape=H.shuriken(), source=NA)
icon(["tailed beast bomb", "bijuu dama", "bijudama", "tailed beast ball"],
     "Tailed Beast Bomb", "Ball", ["Explodes", "Heavy"], 4.0, 200, element="Shadow", accent=BLACK, source=NA)
icon(["sand coffin", "gaara's sand coffin", "sabaku kyu", "sand burial", "desert coffin"],
     "Sand Coffin", "Cage", ["Trap", "Constrict"], 3.0, 400, element="Earth", accent=GOLD, source=NA)
icon(["shinra tensei", "almighty push", "pain's push", "shinra tensai"],
     "Shinra Tensei", "Dome", ["Repel", "Explodes"], 8.0, 10, element="Psychic", accent=PURPLE, source=NA)
icon(["chibaku tensei", "planetary devastation", "chibaku tensai"],
     "Chibaku Tensei", "Ball", ["Attract", "Heavy", "DropFromSky"], 5.0, 800, element="Earth", accent=PURPLE, source=NA)
icon(["kamui", "obito's kamui", "kakashi's kamui", "kamui vortex"],
     "Kamui", "Ball", ["Attract", "Portal", "Spin"], 2.0, 5, element="Psychic", accent=RED, source=NA)
icon(["amaterasu", "black flames", "black fire", "itachi's amaterasu"],
     "Amaterasu", "Staff", ["Beam", "Sticky"], 1.0, 3, element="Fire", accent=BLACK, source=NA)
icon(["gamabunta", "chief toad", "boss toad", "gamabunta toad", "summoning toad"],
     "Gamabunta", "Quadruped", ["Stomp", "Pull", "Heavy"], 4.0, 8000, element="Water", accent=RED,
     temperament="Lazy", intensity=0.4, source=NA)
icon(["fireball jutsu", "great fireball technique", "fire style fireball jutsu", "katon goukakyuu", "goukakyuu no jutsu"],
     "Fireball Jutsu", "Staff", ["Breath"], 1.0, 3, element="Fire", accent=ORANGE, source=NA)

# ======================================================================================= One Piece
OP = "One Piece"
icon(["gum-gum pistol", "gum gum pistol", "gomu gomu no pistol", "gomu gomu pistol", "luffy's punch", "rubber punch", "gum gum punch"],
     "Gum-Gum Pistol", "Gauntlet", ["Blunt", "Returning"], 3.5, 6, accent=RED, shape=H.gum_gum_pistol(), source=OP)
icon(["gear 5", "gear fifth", "gear five", "gear5", "nika", "sun god nika", "luffy gear 5", "gear 5 luffy", "joy boy"],
     "Gear 5", "Gauntlet", ["Blunt", "Bounce", "Grow"], 2.6, 20, accent=WHITE, shape=H.gear5(), source=OP)
icon(["gear 4", "gear fourth", "gear four", "boundman", "bounce man", "kong gun", "gomu gomu no kong gun"],
     "Gear 4", "Gauntlet", ["Blunt", "Bounce", "Heavy"], 2.2, 30, accent=RED, source=OP)
icon(["gum-gum gatling", "gum gum gatling", "gomu gomu no gatling", "gatling punch", "gum gum storm"],
     "Gum-Gum Gatling", "Gauntlet", ["Blunt", "Shoots", "Fast"], 1.2, 6, accent=RED, source=OP)
icon(["thousand sunny", "sunny", "the thousand sunny", "thousand sunny go", "sunny go", "straw hat ship"],
     "Thousand Sunny", "Vehicle", ["Piloted", "Charge", "Shoots"], 8.0, 5000, accent=YELLOW, features=["Sails", "Mast"],
     shape=C.sunny('sunny'), source=OP)
icon(["going merry", "merry", "going merry go", "the going merry"],
     "Going Merry", "Vehicle", ["Piloted", "Charge"], 7.0, 4000, accent=WHITE, features=["Sails", "Mast"],
     shape=C.sunny('merry'), source=OP)
icon(["santoryu", "three sword style", "three-sword style", "zoro's swords", "zoro swords", "zoro's three swords", "oni giri", "onigiri slash"],
     "Santoryu", "Sword", ["Blade", "Spin"], 2.6, 6, accent=WHITE, shape=H.santoryu(), source=OP)
icon(["yoru", "mihawk's sword", "mihawk sword", "black blade yoru", "kokuto yoru"],
     "Yoru", "Sword", ["Blade", "Heavy", "Pierce"], 4.2, 14, accent=GOLD, shape=H.yoru(), source=OP)
icon(["enma", "zoro's enma", "enma sword", "wado ichimonji", "shusui", "sandai kitetsu"],
     "Enma", "Sword", ["Blade", "Pierce"], 1.2, 3, accent=PURPLE, shape=katana('P', 'C', 'A', L=8), source=OP)
icon(["gura gura no mi", "tremor tremor fruit", "quake quake fruit", "whitebeard's quake", "whitebeard punch", "gura gura"],
     "Gura Gura Quake", "Gauntlet", ["Repel", "Blunt", "Heavy"], 1.6, 20, element="Earth", accent=WHITE, source=OP)
icon(["fire fist", "hiken", "ace's fire fist", "fire fist ace", "mera mera no mi", "flame-flame fruit"],
     "Fire Fist", "Gauntlet", ["Shoots", "Heavy"], 1.4, 8, element="Fire", accent=ORANGE, source=OP)
icon(["laboon", "island whale", "laboon whale"],
     "Laboon", "Fish", ["Charge", "Heavy"], 4.0, 20000, element="Water", accent=WHITE, temperament="Playful", intensity=0.5, source=OP)
icon(["kaido dragon", "kaido", "kaidou", "azure dragon kaido", "bolo breath"],
     "Kaido (Dragon)", "Serpent", ["Flies", "Breath", "ArticulatedTail"], 4.0, 8000, element="Fire", accent=BLUE,
     temperament="Aggressive", intensity=0.8, source=OP)
icon(["room shambles", "shambles", "law's room", "ope ope no mi", "op-op fruit", "trafalgar law room"],
     "Room: Shambles", "Dome", ["Portal"], 6.0, 5, element="Psychic", accent=BLUE, source=OP)
icon(["zeus", "nami's zeus", "zeus cloud", "thunder cloud zeus", "clima-tact", "clima tact", "sorcery clima-tact"],
     "Nami's Zeus", "Blob", ["Flies", "Shoots"], 1.6, 20, element="Electric", accent=YELLOW, temperament="Playful", intensity=0.6, source=OP)
icon(["yami yami no mi", "dark dark fruit", "blackbeard's darkness", "black vortex", "liberation", "kurouzu"],
     "Yami Yami Darkness", "Ball", ["Attract", "Explodes"], 3.0, 50, element="Shadow", accent=BLACK, source=OP)

# ======================================================================================= Bleach
BL = "Bleach"
icon(["zangetsu", "ichigo's sword", "ichigo sword", "zangetsu shikai", "shikai zangetsu", "kurosaki zangetsu"],
     "Zangetsu", "Sword", ["Blade", "Heavy"], 2.4, 12, accent=WHITE, shape=H.zangetsu(), source=BL)
icon(["getsuga tensho", "getsuga tenshou", "getsuga", "moon fang heaven piercer", "getsuga tenshō"],
     "Getsuga Tensho", "Sword", ["Blade", "Shoots", "Pierce"], 2.4, 12, element="Shadow", accent=RED, shape=H.zangetsu(True), source=BL)
icon(["tensa zangetsu", "bankai", "ichigo's bankai", "ichigo bankai", "tensa zangetsu bankai"],
     "Tensa Zangetsu", "Sword", ["Blade", "Fast"], 2.2, 3, element="Shadow", accent=RED, shape=H.tensa_zangetsu(), source=BL)
icon(["senbonzakura", "senbonzakura kageyoshi", "byakuya's bankai", "thousand cherry blossoms", "cherry blossom blades"],
     "Senbonzakura", "Insect", ["Blade", "Swarm", "Flies", "Homing"], 0.3, 0.1, count=24, accent=PINK, source=BL)
icon(["hyorinmaru", "hyourinmaru", "daiguren hyorinmaru", "toshiro's bankai", "ice dragon hyorinmaru"],
     "Hyorinmaru", "Serpent", ["Flies", "Breath", "ArticulatedTail"], 4.0, 500, element="Ice", accent=ICE, source=BL)
icon(["ryujin jakka", "ryuujin jakka", "yamamoto's sword", "zanka no tachi"],
     "Ryujin Jakka", "Sword", ["Blade", "Breath"], 1.4, 5, element="Fire", accent=ORANGE, source=BL)
icon(["cero", "gran rey cero", "cero oscuras", "hollow cero"],
     "Cero", "Staff", ["Beam", "Explodes"], 1.0, 3, accent=RED, source=BL)
icon(["kurohitsugi", "black coffin", "hado 90", "hadō 90 kurohitsugi"],
     "Kurohitsugi", "Cage", ["Trap", "Pierce"], 3.0, 200, element="Shadow", accent=BLACK, source=BL)

# ======================================================================================= Jujutsu Kaisen
JJ = "Jujutsu Kaisen"
icon(["hollow purple", "hollow technique purple", "murasaki", "gojo's purple", "purple gojo", "hollow purple gojo"],
     "Hollow Purple", "Cannon", ["Pierce", "Explodes", "Heavy"], 2.0, 30, element="Psychic", accent=PURPLE, source=JJ,
     shape=H.hollow_purple())
icon(["infinity", "gojo's infinity", "limitless", "gojo infinity", "mugen"],
     "Gojo's Infinity", "Dome", ["Barrier", "Repel"], 3.0, 5, element="Psychic", accent=CYAN, source=JJ)
icon(["reversal red", "cursed technique reversal red", "red gojo", "aka", "gojo's red"],
     "Reversal: Red", "Ball", ["Repel", "Explodes", "Fast"], 1.0, 5, element="Psychic", accent=RED, source=JJ)
icon(["lapse blue", "cursed technique lapse blue", "blue gojo", "ao", "gojo's blue"],
     "Lapse: Blue", "Ball", ["Attract"], 1.2, 5, element="Psychic", accent=BLUE, source=JJ)
icon(["unlimited void", "domain expansion unlimited void", "muryokusho", "gojo's domain"],
     "Unlimited Void", "Dome", ["Trap", "Barrier"], 8.0, 10, element="Psychic", accent=CYAN, source=JJ)
icon(["malevolent shrine", "fukuma mizushi", "sukuna's domain", "sukuna domain", "domain expansion malevolent shrine"],
     "Malevolent Shrine", "Pillar", ["Blade", "Shoots", "Barrier"], 5.0, 3000, element="Shadow", accent=RED,
     shape=H.malevolent_shrine(), source=JJ)
icon(["dismantle", "cleave", "sukuna's dismantle", "sukuna's cleave", "kai", "hachi"],
     "Dismantle", "Disc", ["Blade", "Pierce", "Fast"], 1.5, 1, element="Shadow", accent=RED, source=JJ)
icon(["fuga", "furnace", "divine flame", "open fuga", "sukuna's fire arrow", "fire arrow sukuna"],
     "Fuga", "Bow", ["Shoots", "Explodes", "Heavy"], 1.6, 5, element="Fire", accent=ORANGE, source=JJ)
icon(["mahoraga", "divine general mahoraga", "eight-handled sword divergent sila divine general mahoraga", "makora", "eight handled wheel"],
     "Mahoraga", "Biped", ["Blade", "Heavy", "Charge"], 4.0, 3000, accent=GOLD, shape=C.mahoraga(),
     temperament="Aggressive", intensity=0.9, source=JJ)
icon(["divine dogs", "divine dog", "megumi's dogs", "gyokuken", "divine dog totality"],
     "Divine Dogs", "Quadruped", ["Charge", "Fast"], 1.2, 60, count=2, element="Shadow", accent=WHITE,
     temperament="Aggressive", intensity=0.7, source=JJ)
icon(["nue", "megumi's nue", "nue shikigami"],
     "Nue", "Bird", ["Flies", "DropFromSky"], 1.6, 20, element="Electric", accent=WHITE, source=JJ)
icon(["rika", "rika orimoto", "queen of curses", "yuta's rika"],
     "Rika", "Biped", ["Grab", "Heavy"], 4.0, 1000, element="Shadow", accent=WHITE, temperament="Protective", intensity=0.9, source=JJ)
icon(["black flash", "kokusen", "yuji's black flash", "itadori black flash"],
     "Black Flash", "Gauntlet", ["Blunt", "Heavy"], 1.0, 8, element="Shadow", accent=RED, source=JJ)
icon(["playful cloud", "three section staff", "three-section staff", "toji's staff"],
     "Playful Cloud", "Staff", ["Blunt", "Heavy", "Fast"], 2.0, 8, accent=RED, source=JJ)
icon(["inverted spear of heaven", "inverted spear", "toji's spear", "tenketsu no sakasaboko"],
     "Inverted Spear of Heaven", "Spear", ["Pierce", "Fast"], 1.2, 2, accent=PURPLE, source=JJ)

# ======================================================================================= Demon Slayer
DS = "Demon Slayer"
icon(["nichirin sword", "nichirin", "nichirin blade", "demon slayer sword", "tanjiro's sword", "tanjiro sword", "black nichirin"],
     "Nichirin Sword", "Sword", ["Blade", "Fast"], 1.3, 2, accent=RED, shape=katana('D', 'C', 'A', L=6), source=DS)
icon(["water breathing", "water breathing sword", "water surface slash", "constant flux", "tanjiro water breathing"],
     "Water Breathing", "Sword", ["Blade", "Fast"], 1.3, 2, element="Water", accent=BLUE, shape=katana('A', 'C', 'D', L=6), source=DS)
icon(["hinokami kagura", "sun breathing", "dance of the fire god", "hinokami", "sun breathing sword"],
     "Hinokami Kagura", "Sword", ["Blade", "Spin"], 1.3, 2, element="Fire", accent=ORANGE, shape=katana('A', 'C', 'D', L=6), source=DS)
icon(["thunder breathing", "thunderclap and flash", "thunder breathing first form", "zenitsu's sword", "godspeed zenitsu", "zenitsu"],
     "Thunderclap and Flash", "Sword", ["Blade", "Thrust", "Fast"], 1.3, 2, element="Electric", accent=YELLOW,
     shape=katana('A', 'C', 'D', L=6), source=DS)
icon(["flame breathing", "rengoku's sword", "flame hashira sword", "purgatory", "ninth form rengoku", "unknowing fire"],
     "Flame Breathing", "Sword", ["Blade", "Charge", "Heavy"], 1.4, 3, element="Fire", accent=RED, shape=katana('A', 'C', 'D', L=7), source=DS)
icon(["beast breathing", "inosuke's swords", "inosuke's blades", "serrated nichirin blades"],
     "Beast Breathing", "Sword", ["Blade", "Fast"], 1.0, 2, count=2, accent=BLUE, source=DS)
icon(["tengen's cleavers", "sound breathing", "uzui's blades", "chained cleavers"],
     "Sound Breathing Cleavers", "Sword", ["Blade", "Explodes", "Link"], 1.6, 8, accent=GOLD, features=["Chains"], source=DS)
icon(["moon breathing", "kokushibo's sword", "kokushibo sword", "moon breathing blade"],
     "Moon Breathing", "Sword", ["Blade", "Shoots"], 3.0, 10, element="Shadow", accent=PURPLE, source=DS)

# ======================================================================================= Chainsaw Man
CM = "Chainsaw Man"
icon(["pochita", "chainsaw devil", "pochita dog", "chainsaw dog"],
     "Pochita", "Quadruped", ["Blade", "Charge"], 0.8, 10, accent=ORANGE, features=["Saw"], shape=C.pochita(),
     temperament="Playful", intensity=0.8, source=CM)
icon(["chainsaw man", "denji", "chainsawman", "chainsaw-man", "denji chainsaw"],
     "Chainsaw Man", "Biped", ["Blade", "Link", "Charge"], 2.2, 90, accent=ORANGE, features=["Saw", "Chains"], shape=C.chainsaw_man(),
     temperament="Aggressive", intensity=0.9, source=CM)
icon(["blood hammer", "power blood hammer", "blood fiend hammer"],
     "Blood Hammer", "Hammer", ["Blunt", "Heavy"], 1.8, 25, accent=RED, source=CM)
icon(["bomb devil", "reze", "bomb girl", "reze bomb devil", "bomb hybrid"],
     "Bomb Devil", "Biped", ["Explodes", "Fast"], 2.0, 60, element="Fire", accent=ORANGE, temperament="Aggressive", intensity=0.8, source=CM)
icon(["katana man", "samurai sword devil", "sword devil"],
     "Katana Man", "Biped", ["Blade", "Charge"], 2.0, 80, accent=WHITE, temperament="Aggressive", intensity=0.7, source=CM)

# ======================================================================================= Attack on Titan
AT = "Attack on Titan"
icon(["odm gear", "omni-directional mobility gear", "3d maneuver gear", "3dmg", "odm", "vertical maneuvering equipment", "maneuver gear"],
     "ODM Gear", "Jetpack", ["Thrust", "Pull", "Fast"], 1.0, 12, accent=WHITE, shape=H.odm_gear(), source=AT)
icon(["thunder spear", "thunder spears", "anti-personnel thunder spear", "thunderspear"],
     "Thunder Spear", "Rocket", ["Pierce", "Sticky", "Explodes"], 1.4, 6, count=2, accent=YELLOW, shape=H.thunder_spear(), source=AT)
icon(["colossal titan", "colossal", "bertholdt titan", "armin's titan", "colossus titan"],
     "Colossal Titan", "Kaiju", ["Stomp", "Heavy", "Repel"], 20.0, 60000, element="Fire", accent=RED, shape=C.titan('colossal'),
     temperament="Lazy", intensity=0.3, source=AT)
icon(["armored titan", "armoured titan", "reiner's titan", "armored"],
     "Armored Titan", "Kaiju", ["Charge", "Barrier", "Heavy"], 15.0, 45000, accent=GOLD, features=["Armor"], shape=C.titan('armored'),
     temperament="Aggressive", intensity=0.7, source=AT)
icon(["attack titan", "eren's titan", "eren titan", "rogue titan", "eren yeager titan"],
     "Attack Titan", "Kaiju", ["Piloted", "Blunt", "Charge"], 15.0, 40000, accent=GREEN, shape=C.titan('attack'), source=AT)
icon(["beast titan", "zeke's titan", "zeke titan", "ape titan"],
     "Beast Titan", "Kaiju", ["Shoots", "Heavy"], 17.0, 40000, element="Earth", accent=WHITE, shape=C.great_ape(),
     temperament="Fearful", intensity=0.6, source=AT,
     payload=dict(name="thrown boulder", form="Boulder", mech=["Heavy"], element="Earth", scale=1.5, mass=300))
icon(["the rumbling", "rumbling", "wall titans", "wall titan", "founding titan rumbling"],
     "The Rumbling", "Kaiju", ["Stomp", "Swarm", "Heavy"], 16.0, 50000, count=5, element="Fire", accent=RED,
     shape=C.titan('colossal'), temperament="Chaotic", intensity=1.0, source=AT)
icon(["wall maria", "wall rose", "wall sina", "the walls", "paradis wall"],
     "Wall Maria", "Wall", ["Barrier", "Heavy"], 12.0, 50000, accent=WHITE, source=AT)
icon(["odm blades", "ultrahard steel blades", "ultra hard steel blades", "scout blades", "survey corps blades"],
     "ODM Blades", "Sword", ["Blade", "Fast"], 1.1, 2, count=2, accent=WHITE, source=AT)
icon(["female titan", "annie's titan", "annie titan"],
     "Female Titan", "Kaiju", ["Fast", "Guard", "Blunt"], 14.0, 30000, accent=ICE, temperament="Sneaky", intensity=0.6, source=AT)
icon(["cart titan", "pieck's titan", "panzer cart titan"],
     "Cart Titan", "Quadruped", ["Piloted", "Shoots", "Fast"], 4.0, 8000, features=["Turret"], accent=WHITE, source=AT)

# ======================================================================================= My Hero Academia
MH = "My Hero Academia"
icon(["detroit smash", "one for all", "deku smash", "detroit", "ofa punch", "full cowl smash"],
     "Detroit Smash", "Gauntlet", ["Blunt", "Repel", "Heavy"], 1.2, 10, element="Wind", accent=GREEN, source=MH)
icon(["united states of smash", "all might punch", "all might smash", "plus ultra punch", "all might"],
     "United States of Smash", "Gauntlet", ["Blunt", "Heavy", "Repel"], 2.0, 30, element="Wind", accent=YELLOW, source=MH)
icon(["bakugo's gauntlet", "bakugo gauntlet", "grenade bracer", "bakugo's grenade gauntlet", "katsuki's gauntlet", "explosion gauntlet"],
     "Bakugo's Grenade Gauntlet", "Gauntlet", ["Explodes", "Blunt"], 1.0, 6, element="Fire", accent=ORANGE, shape=H.bakugo_gauntlet(), source=MH)
icon(["howitzer impact", "bakugo howitzer", "howitzer impact bakugo", "ap shot"],
     "Howitzer Impact", "Gauntlet", ["Explodes", "Spin", "Heavy"], 1.2, 8, element="Fire", accent=ORANGE, source=MH)
icon(["todoroki's ice wall", "todoroki ice wall", "heaven-piercing ice wall", "heaven piercing ice wall", "todoroki ice"],
     "Heaven-Piercing Ice Wall", "Wall", ["Barrier"], 8.0, 3000, element="Ice", accent=ICE, source=MH)
icon(["dark shadow", "tokoyami's dark shadow", "tokoyami shadow", "black ankh"],
     "Dark Shadow", "Bird", ["Flies", "Grab"], 2.4, 40, element="Shadow", accent=BLACK, temperament="Protective", intensity=0.8, source=MH)
icon(["fierce wings", "hawks' feathers", "hawks feathers", "hawks' wings", "feather swords"],
     "Fierce Wings", "Bird", ["Blade", "Swarm", "Homing", "Flies"], 0.4, 0.2, count=16, element="Fire", accent=RED, source=MH)
icon(["uraraka's meteor shower", "meteor shower", "comet home run", "uraraka's meteor shower", "meteor rain"],
     "Uraraka's Meteor Shower", "Boulder", ["DropFromSky", "Swarm", "Heavy"], 1.2, 80, count=6, element="Earth", accent=PINK, source=MH)
icon(["grape rush", "pop off", "mineta's grapes", "grape balls", "sticky grapes"],
     "Pop Off", "Ball", ["Sticky", "Bounce"], 0.4, 1, count=6, accent=PURPLE, source=MH)
icon(["prominence burn", "endeavor's prominence burn", "hell flame", "flashfire fist"],
     "Prominence Burn", "Staff", ["Beam", "Heavy"], 1.4, 5, element="Fire", accent=ORANGE, source=MH)

# ======================================================================================= Hunter x Hunter
HX = "Hunter x Hunter"
icon(["killua's yo-yo", "killua's yoyo", "killua yoyo", "killua's yo-yos", "yo-yo killua", "killua yo yo"],
     "Killua's Yo-yo", "Disc", ["Blunt", "Heavy"], 1.0, 50, count=2, element="Electric", accent=CYAN, shape=H.killua_yoyo(), source=HX)
icon(["bungee gum", "hisoka's bungee gum", "hisoka gum", "bungee"],
     "Bungee Gum", "Whip", ["Sticky", "Pull", "Bounce"], 3.0, 1, accent=PINK, source=HX)
icon(["chain jail", "kurapika's chains", "kurapika chain", "judgment chain", "dowsing chain", "holy chain"],
     "Chain Jail", "Whip", ["Link", "Constrict", "Trap"], 3.0, 4, accent=GOLD, features=["Chains"], source=HX)
icon(["hundred-type guanyin bodhisattva", "hundred type guanyin bodhisattva", "guanyin bodhisattva", "netero's bodhisattva", "netero statue",
      "zero hand", "100-type guanyin bodhisattva"],
     "Hundred-Type Guanyin Bodhisattva", "Kaiju", ["Blunt", "Fast", "DropFromSky"], 14.0, 20000, accent=GOLD, shape=C.guanyin(), source=HX)
icon(["jajanken", "jajanken rock", "gon's rock", "gon's jajanken", "rock paper scissors punch"],
     "Jajanken", "Gauntlet", ["Blunt", "Explodes", "Heavy"], 1.0, 8, accent=GREEN, source=HX)
icon(["godspeed", "killua godspeed", "whirlwind killua", "thunderbolt killua"],
     "Killua's Godspeed", "Cape", ["Fast", "Charge"], 1.2, 2, element="Electric", accent=CYAN, source=HX)

# ======================================================================================= Evangelion
EV = "Neon Genesis Evangelion"
icon(["eva unit-01", "eva unit 01", "unit-01", "unit 01", "evangelion", "eva 01", "eva-01", "evangelion unit 01", "eva"],
     "EVA Unit-01", "Kaiju", ["Piloted", "Grab", "Blunt"], 20.0, 30000, accent=GREEN, features=["Armor"], shape=C.eva01(),
     temperament="Aggressive", intensity=0.9, source=EV)
icon(["lance of longinus", "spear of longinus", "longinus", "lance of cassius", "longinus spear"],
     "Lance of Longinus", "Spear", ["Pierce", "Homing", "Returning"], 5.0, 30, accent=RED, shape=H.lance_of_longinus(), source=EV)
icon(["at field", "a.t. field", "absolute terror field", "at-field", "a t field"],
     "AT Field", "Wall", ["Barrier", "Repel"], 5.0, 10, accent=ORANGE, shape=H.at_field(), source=EV)
icon(["ramiel", "fifth angel", "5th angel", "ramiel angel", "the octahedron angel"],
     "Ramiel", "Blob", ["Flies", "Beam", "Barrier"], 4.0, 5000, accent=BLUE, shape=C.ramiel(), temperament="Fearful", intensity=0.5, source=EV)
icon(["positron rifle", "positron cannon", "operation yashima rifle"],
     "Positron Rifle", "Cannon", ["Beam", "Heavy"], 4.0, 200, accent=BLUE, source=EV)
icon(["progressive knife", "prog knife", "eva knife"],
     "Progressive Knife", "Sword", ["Blade", "Pierce"], 1.2, 3, accent=CYAN, source=EV)

# ======================================================================================= Gundam / mecha
GU = "Mobile Suit Gundam"
icon(["gundam", "rx-78-2", "rx-78-2 gundam", "rx 78 2", "rx78", "gundam rx-78", "mobile suit gundam", "amuro's gundam", "giant robot"],
     "RX-78-2 Gundam", "Kaiju", ["Piloted", "Shoots", "Guard"], 18.0, 40000, accent=YELLOW, features=["Cockpit", "Jet", "Antenna"],
     shape=C.mech('gundam'), source=GU)
icon(["beam saber", "beam sabre", "gundam saber", "gundam beam saber"],
     "Beam Saber", "Sword", ["Blade"], 1.6, 2, element="Fire", accent=PINK, shape=H.beam_saber(), source=GU)
icon(["fin funnels", "fin funnel", "funnels", "gundam funnels", "dragoons", "gund bits", "gund-bits"],
     "Fin Funnels", "Rocket", ["Beam", "Swarm", "Flies"], 0.8, 5, count=6, accent=YELLOW, source=GU)
icon(["zaku", "zaku ii", "zaku 2", "char's zaku", "red comet", "char's custom zaku"],
     "Zaku II", "Kaiju", ["Piloted", "Shoots", "Blunt"], 17.5, 50000, accent=RED, features=["Cockpit", "Pipes"], source=GU)
icon(["gundam barbatos", "barbatos", "barbatos lupus rex", "barbatos mace"],
     "Gundam Barbatos", "Kaiju", ["Piloted", "Blunt", "ArticulatedTail"], 18.0, 30000, accent=WHITE, source="Iron-Blooded Orphans")
icon(["wing gundam zero", "wing zero", "gundam wing zero", "twin buster rifle"],
     "Wing Gundam Zero", "Kaiju", ["Piloted", "Flies", "Beam"], 16.5, 8000, accent=WHITE, features=["Jet", "Cockpit"], source="Gundam Wing")
GL = "Gurren Lagann"
icon(["gurren lagann", "gurren-lagann", "tengen toppa gurren lagann", "ttgl", "gurren", "team dai-gurren"],
     "Gurren Lagann", "Kaiju", ["Piloted", "Spin", "Pierce"], 14.0, 30000, accent=RED, features=["Drill"], shape=C.gurren_lagann(), source=GL)
icon(["giga drill", "giga drill break", "giga drill breaker", "gigadrill", "drill that pierces the heavens", "tengen toppa drill"],
     "Giga Drill Break", "Spear", ["Spin", "Pierce", "Heavy"], 4.0, 60, accent=RED, features=["Drill"], shape=H.giga_drill(), source=GL)
icon(["simon's core drill", "core drill", "simon's drill", "simon's core drill"],
     "Simon's Core Drill", "Spear", ["Spin", "Pierce"], 0.5, 0.5, accent=GREEN, features=["Drill"], source=GL)
MZ = "Mazinger Z"
icon(["rocket punch", "rocketto panchi", "mazinger rocket punch", "flying fist", "rocket fist"],
     "Rocket Punch", "Gauntlet", ["Returning", "Thrust", "Blunt"], 1.4, 20, accent=RED, shape=H.rocket_punch(), source=MZ)
icon(["mazinger z", "mazinger", "mazinger-z", "super robot", "mazinkaiser"],
     "Mazinger Z", "Kaiju", ["Piloted", "Breath", "Blunt"], 18.0, 20000, element="Fire", accent=RED, features=["Cockpit"],
     shape=C.mech('mazinger'), source=MZ)
icon(["valkyrie", "vf-1 valkyrie", "vf-1", "macross valkyrie", "veritech"],
     "VF-1 Valkyrie", "Vehicle", ["Flies", "Piloted", "Shoots"], 14.0, 13000, accent=WHITE, features=["Jet", "Cockpit"], source="Macross")
icon(["voltron", "golion", "beast king golion", "blazing sword voltron"],
     "Voltron", "Kaiju", ["Piloted", "Blade", "Stomp"], 20.0, 50000, accent=BLUE, source="Beast King GoLion")

# ======================================================================================= Ghibli
GH = "Studio Ghibli"
icon(["totoro", "my neighbor totoro", "big totoro", "o-totoro", "ototoro", "my neighbour totoro"],
     "Totoro", "Biped", ["Stomp", "Heavy", "SlowFall"], 2.8, 300, accent=WHITE, shape=C.totoro(),
     temperament="Lazy", intensity=0.6, source=GH)
icon(["catbus", "cat bus", "nekobasu", "neko bus", "cat-bus"],
     "Catbus", "Quadruped", ["Piloted", "Fast", "Charge"], 3.0, 800, accent=ORANGE, features=["Windows", "Headlight"], shape=C.catbus(),
     temperament="Playful", intensity=0.8, source=GH)
icon(["soot sprites", "soot sprite", "susuwatari", "soot gremlins", "dust bunnies ghibli", "makkuro kurosuke"],
     "Soot Sprites", "Blob", ["Swarm", "Sticky"], 0.3, 0.1, count=12, element="Shadow", accent=WHITE, shape=C.soot_sprite(),
     temperament="Playful", intensity=0.7, source=GH)
icon(["no-face", "no face", "kaonashi", "noface", "no face spirit"],
     "No-Face", "Biped", ["Grab", "Attract"], 2.6, 60, element="Shadow", accent=WHITE, shape=C.no_face(),
     temperament="Sneaky", intensity=0.7, source=GH)
icon(["calcifer", "calcifer fire demon", "howl's fire demon"],
     "Calcifer", "Blob", ["Flies", "Breath"], 0.6, 1, element="Fire", accent=ORANGE, shape=C.calcifer(),
     temperament="Playful", intensity=0.6, source=GH)
icon(["howl's moving castle", "moving castle", "howls moving castle", "howl's castle", "walking castle"],
     "Howl's Moving Castle", "Vehicle", ["Walks", "Piloted", "Heavy", "Shoots"], 10.0, 30000, accent=GOLD,
     features=["Smokestack", "Pipes", "Windows"], shape=C.howls_castle(), source=GH)
icon(["laputa robot", "robot soldier", "laputa robot soldier", "castle in the sky robot", "laputan robot"],
     "Laputa Robot Soldier", "Biped", ["Beam", "Guard"], 3.5, 800, accent=GREEN, shape=C.laputa_robot(),
     temperament="Protective", intensity=0.8, source=GH)
icon(["kiki's broom", "kikis broom", "witch's broom kiki", "kiki broom", "kiki's delivery broom"],
     "Kiki's Broom", "Staff", ["Thrust", "Glide"], 1.6, 2, accent=RED, shape=H.kiki_broom(), source=GH)
icon(["haku", "haku dragon", "kohaku river", "spirited away dragon", "nigihayami kohakunushi"],
     "Haku", "Serpent", ["Flies", "Fast"], 4.0, 300, element="Water", accent=WHITE, temperament="Protective", intensity=0.7, source=GH)
icon(["ohmu", "ohm", "nausicaa ohmu", "o-mu"],
     "Ohmu", "Insect", ["Charge", "Heavy"], 4.0, 20000, accent=BLUE, temperament="Lazy", intensity=0.5, source=GH)
icon(["moro", "wolf god moro", "moro wolf", "mononoke wolf", "princess mononoke wolf"],
     "Moro", "Quadruped", ["Charge", "Grab"], 3.0, 900, accent=WHITE, temperament="Aggressive", intensity=0.7, source=GH)
icon(["mehve", "nausicaa's glider", "nausicaa glider", "moeve"],
     "Mehve", "Wings", ["Glide", "Thrust"], 3.0, 15, accent=WHITE, features=["Jet"], source=GH)

# ======================================================================================= Sailor Moon / magical girls
SM = "Sailor Moon"
icon(["moon tiara", "moon tiara action", "moon tiara boomerang", "sailor moon tiara", "tiara action"],
     "Moon Tiara", "Disc", ["Blade"], 1.0, 1, accent=GOLD, shape=H.moon_tiara(), source=SM)
icon(["cutie moon rod", "moon stick", "moon rod", "spiral heart moon rod", "sailor moon wand", "moon healing escalation", "crescent moon wand"],
     "Cutie Moon Rod", "Staff", ["Beam", "Shoots"], 1.4, 1, element="Psychic", accent=PINK, shape=H.moon_rod(), source=SM)
icon(["starlight breaker", "nanoha's starlight breaker", "raising heart", "divine buster"],
     "Starlight Breaker", "Staff", ["Beam", "Explodes"], 1.6, 3, accent=PINK, source="Magical Girl Lyrical Nanoha")

# ======================================================================================= Pokemon anime moves
PK = "Pokemon (anime)"
icon(["volt tackle", "pikachu's volt tackle", "volt crash"],
     "Volt Tackle", "Quadruped", ["Charge", "Fast"], 0.8, 6, element="Electric", accent=YELLOW, temperament="Aggressive", intensity=0.8, source=PK)
icon(["hyper beam", "hyperbeam", "giga impact beam"],
     "Hyper Beam", "Cannon", ["Beam", "Heavy"], 1.6, 10, accent=ORANGE, source=PK)
icon(["solar beam", "solarbeam", "solar beam attack"],
     "Solar Beam", "Staff", ["Beam"], 1.4, 3, element="Fire", accent=YELLOW, source=PK)
icon(["hydro pump", "hydropump", "water gun pokemon"],
     "Hydro Pump", "Cannon", ["Beam", "Repel"], 1.4, 8, element="Water", accent=BLUE, source=PK)
icon(["iron tail", "iron tail attack"],
     "Iron Tail", "Whip", ["Blunt", "Heavy"], 1.6, 10, accent=WHITE, source=PK)
icon(["team rocket balloon", "meowth balloon", "team rocket's balloon", "meowth hot air balloon"],
     "Team Rocket Balloon", "Vehicle", ["Flies", "SlowFall", "Piloted"], 5.0, 200, accent=WHITE, source=PK)

# ======================================================================================= Frieren
FR = "Frieren: Beyond Journey's End"
icon(["zoltraak", "frieren's staff", "frieren staff", "killing magic", "ordinary offensive magic", "zoltraak spell", "frieren"],
     "Zoltraak", "Staff", ["Beam", "Pierce"], 1.8, 2, accent=GOLD, shape=H.frieren_staff(), source=FR)
icon(["defensive magic", "frieren barrier", "defense magic", "hexagon barrier"],
     "Defensive Magic", "Dome", ["Barrier", "Guard"], 3.0, 5, accent=CYAN, source=FR)
icon(["mimic", "mimic chest", "treasure chest mimic", "frieren mimic", "mimic box"],
     "Frieren's Mimic", "Blob", ["Grab", "Trap"], 1.2, 60, accent=GOLD, shape=C.mimic(), temperament="Sneaky", intensity=0.8, source=FR)
icon(["judradjim", "jubelade", "lightning magic frieren", "judradjim lightning"],
     "Judradjim", "Staff", ["DropFromSky", "Beam"], 1.6, 2, element="Electric", accent=CYAN, source=FR)
icon(["stark's axe", "stark axe", "lightning strike stark"],
     "Stark's Axe", "Axe", ["Heavy", "Blunt", "Stomp"], 1.8, 12, accent=RED, source=FR)

# ======================================================================================= Dandadan / Solo Leveling / Kaiju No. 8 / Spy x Family
icon(["turbo granny", "turbo baba", "turbo-granny", "turbo babaa"],
     "Turbo Granny", "Biped", ["Fast", "Charge"], 1.4, 40, accent=WHITE, temperament="Aggressive", intensity=0.9, source="Dandadan")
icon(["momo's psychokinesis", "psychokinesis hands", "momo's aura hands", "momo psychic hands", "ghost hands"],
     "Momo's Psychokinesis", "Gauntlet", ["Grab", "Pull"], 2.0, 5, element="Psychic", accent=CYAN, source="Dandadan")
icon(["great kinta", "kinta's robot", "great kinta mech"],
     "Great Kinta", "Kaiju", ["Piloted", "Shoots", "Stomp"], 12.0, 20000, accent=YELLOW, features=["Cockpit"], source="Dandadan")
icon(["arise", "shadow army", "shadow soldiers", "shadow monarch", "sung jinwoo's army", "shadow extraction", "igris"],
     "Shadow Army", "Biped", ["Swarm", "Blade"], 2.0, 80, count=6, element="Shadow", accent=PURPLE, shape=C.shadow_soldier(),
     temperament="Aggressive", intensity=0.8, source="Solo Leveling")
icon(["beru", "ant king beru", "shadow beru"],
     "Beru", "Insect", ["Blade", "Fast", "Flies"], 2.4, 150, element="Shadow", accent=PURPLE, temperament="Aggressive", intensity=1.0, source="Solo Leveling")
icon(["kaisel", "shadow wyvern", "jinwoo's dragon"],
     "Kaisel", "Bird", ["Flies", "Piloted", "Breath"], 4.0, 1000, element="Shadow", accent=PURPLE, source="Solo Leveling")
icon(["kamish's wrath", "kamish wrath", "jinwoo's daggers", "demon king's daggers", "knight killer"],
     "Kamish's Wrath", "Sword", ["Blade", "Fast", "Pierce"], 0.8, 1, count=2, element="Shadow", accent=PURPLE, source="Solo Leveling")
icon(["kaiju no. 8", "kaiju no 8", "kaiju number 8", "kaiju 8", "kafka hibino", "kaijuu 8"],
     "Kaiju No. 8", "Biped", ["Blunt", "Heavy", "Charge"], 2.6, 300, accent=WHITE, shape=C.kaiju8(),
     temperament="Aggressive", intensity=0.8, source="Kaiju No. 8")
icon(["kaiju no. 9", "kaiju no 9", "kaiju 9"],
     "Kaiju No. 9", "Biped", ["Grab", "Shoots"], 2.4, 200, element="Shadow", accent=WHITE, temperament="Sneaky", intensity=0.8, source="Kaiju No. 8")
icon(["bond forger", "bond", "bond dog", "forger family dog"],
     "Bond Forger", "Quadruped", ["Charge", "Heavy"], 1.4, 50, accent=WHITE, temperament="Protective", intensity=0.9, source="Spy x Family")
icon(["yor's stilettos", "thorn princess", "yor forger's needles", "thorn princess stilettos"],
     "Thorn Princess Stilettos", "Sword", ["Pierce", "Fast"], 0.6, 1, count=2, accent=RED, source="Spy x Family")

# ======================================================================================= JoJo / FMA / One Punch / others
JO = "JoJo's Bizarre Adventure"
icon(["star platinum", "jotaro's stand", "star platinum stand", "ora ora rush"],
     "Star Platinum", "Biped", ["Blunt", "Fast", "Guard"], 2.2, 90, element="Psychic", accent=PURPLE, shape=C.star_platinum(),
     temperament="Protective", intensity=0.9, source=JO)
icon(["the world stand", "za warudo", "dio's stand", "the world jojo"],
     "The World", "Biped", ["Blunt", "Fast"], 2.2, 90, element="Psychic", accent=GOLD, temperament="Protective", intensity=0.9, source=JO)
icon(["dio's road roller", "road roller", "road roller da", "dio's road roller", "roller da", "steamroller"],
     "Dio's Road Roller", "Vehicle", ["DropFromSky", "Heavy", "Blunt"], 4.0, 8000, accent=YELLOW, features=["Wheels", "Cab"],
     shape=H.road_roller(), source=JO)
icon(["steel ball", "gyro's steel ball", "steel balls", "gyro steel ball", "zeppeli steel ball", "spin technique"],
     "Gyro's Steel Ball", "Ball", ["Spin", "Ricochet"], 0.3, 1, count=2, accent=GREEN, source=JO)
icon(["tusk act 4", "tusk", "johnny's tusk", "infinite rotation"],
     "Tusk Act 4", "Gun", ["Spin", "Homing", "Pierce"], 0.8, 2, accent=PINK, source=JO)
FM = "Fullmetal Alchemist"
icon(["alphonse elric", "alphonse", "al elric", "alphonse armor", "al's armor"],
     "Alphonse Elric", "Biped", ["Guard", "Heavy"], 2.3, 200, accent=RED, features=["Armor"], shape=C.alphonse(),
     temperament="Protective", intensity=0.8, source=FM)
icon(["flame alchemy", "roy mustang's snap", "mustang's glove", "flame alchemist", "mustang snap", "roy mustang"],
     "Flame Alchemy", "Gauntlet", ["Explodes", "Shoots"], 0.8, 1, element="Fire", accent=WHITE, source=FM)
icon(["ed's alchemy", "alchemy wall", "earth alchemy", "transmutation wall", "edward's alchemy"],
     "Earth Alchemy", "Wall", ["Barrier", "Pierce"], 4.0, 2000, element="Earth", accent=BLUE, source=FM)
icon(["automail blade", "automail arm", "ed's automail", "edward's arm blade"],
     "Automail Blade", "Gauntlet", ["Blade", "Pierce"], 1.0, 8, accent=WHITE, source=FM)
OPM = "One-Punch Man"
icon(["serious punch", "saitama's punch", "saitama punch", "one punch", "serious series", "killer move serious punch"],
     "Serious Punch", "Gauntlet", ["Blunt", "Heavy", "Repel"], 1.0, 10, element="Wind", accent=RED, source=OPM)
icon(["consecutive normal punches", "normal punch", "saitama barrage"],
     "Consecutive Normal Punches", "Gauntlet", ["Blunt", "Fast", "Shoots"], 1.0, 6, accent=RED, source=OPM)
icon(["incinerate cannon", "genos' incinerate", "genos cannon", "incineration cannon"],
     "Incinerate Cannon", "Cannon", ["Beam", "Heavy"], 1.4, 30, element="Fire", accent=ORANGE, source=OPM)
icon(["tatsumaki", "tornado of terror", "tatsumaki's psychic"],
     "Tatsumaki", "Dome", ["Attract", "Repel"], 6.0, 10, element="Psychic", accent=GREEN, source=OPM)
icon(["mob 100%", "mob psycho 100%", "???%", "mob 100 percent", "mob explosion"],
     "Mob 100%", "Dome", ["Repel", "Explodes"], 8.0, 10, element="Psychic", accent=WHITE, source="Mob Psycho 100")

# ======================================================================================= Doraemon
DO = "Doraemon"
icon(["anywhere door", "dokodemo door", "doraemon door", "dokodemo doa", "anywhere-door"],
     "Anywhere Door", "Wall", ["Portal"], 2.5, 30, accent=PINK, shape=H.anywhere_door(), source=DO)
icon(["small light", "doraemon small light", "shrink light", "smaller light", "shrinking flashlight"],
     "Small Light", "Gun", ["Shrink"], 0.8, 1, accent=CYAN, shape=H.small_light(), source=DO)
icon(["big light", "doraemon big light", "grow light", "bigger light", "big lite", "growing flashlight"],
     "Big Light", "Gun", ["Grow"], 0.9, 1, accent=YELLOW, shape=H.small_light(True), source=DO)
icon(["take-copter", "takecopter", "bamboo copter", "bamboo-copter", "doraemon's copter", "hopter", "take copter"],
     "Take-copter", "Jetpack", ["Thrust", "SlowFall"], 0.5, 0.5, accent=YELLOW, features=["Rotor"], source=DO)
icon(["air cannon", "doraemon's air cannon", "air cannon doraemon", "kuki hou"],
     "Air Cannon", "Gauntlet", ["Repel", "Shoots"], 0.9, 2, element="Wind", accent=CYAN, source=DO)

# ======================================================================================= Classics & cult
icon(["kaneda's bike", "kaneda bike", "akira bike", "kaneda's motorcycle", "akira motorcycle", "kaneda slide"],
     "Kaneda's Bike", "Vehicle", ["Piloted", "Fast", "Charge"], 3.0, 300, accent=RED, features=["Wheels", "Headlight"],
     shape=C.kaneda_bike(), source="Akira")
icon(["tachikoma", "tachikomas", "think tank tachikoma", "fuchikoma"],
     "Tachikoma", "Arachnid", ["Piloted", "Shoots", "Fast"], 2.8, 2000, accent=BLUE, features=["Turret"], shape=C.tachikoma(),
     temperament="Playful", intensity=0.7, source="Ghost in the Shell")
icon(["dragonslayer", "dragon slayer", "guts' sword", "guts sword", "gut's sword", "dragon slayer sword", "berserk sword"],
     "Dragonslayer", "Sword", ["Blade", "Heavy", "Blunt"], 4.0, 90, accent=WHITE, shape=H.dragonslayer(), source="Berserk")
icon(["guts' cannon arm", "cannon arm", "guts cannon arm", "prosthetic cannon"],
     "Guts' Cannon Arm", "Cannon", ["Explodes"], 1.0, 20, accent=WHITE, source="Berserk")
icon(["scissor blade", "ryuko's scissor blade", "scissor sword", "rending scissors", "decapitation mode"],
     "Scissor Blade", "Sword", ["Blade", "Pierce"], 2.8, 8, accent=RED, shape=H.scissor_blade(), source="Kill la Kill")
icon(["tessaiga", "tetsusaiga", "inuyasha's sword", "inuyasha sword", "wind scar", "kaze no kizu"],
     "Tessaiga", "Sword", ["Blade", "Heavy", "Shoots"], 3.0, 15, element="Wind", accent=WHITE, shape=H.tessaiga(), source="Inuyasha")
icon(["asta's sword", "anti-magic sword", "demon dweller sword", "demon-dweller sword", "asta sword", "demon destroyer sword"],
     "Anti-Magic Sword", "Sword", ["Blade", "Heavy", "Guard"], 3.2, 40, element="Shadow", accent=WHITE, shape=H.asta_sword(), source="Black Clover")
icon(["sakabato", "reverse blade sword", "reverse-blade sword", "kenshin's sword", "sakabatou", "kenshin sword"],
     "Sakabato", "Sword", ["Blunt", "Fast"], 1.3, 2, accent=RED, shape=katana('P', 'C', 'A', L=7, reverse=True), source="Rurouni Kenshin")
icon(["storm pegasus beyblade", "beyblade", "beyblades", "let it rip", "pegasus beyblade", "dragoon beyblade", "spinning top battle"],
     "Storm Pegasus Beyblade", "Disc", ["Spin", "Ricochet", "Charge"], 0.8, 1, accent=BLUE, shape=H.beyblade(), source="Beyblade")
icon(["blue-eyes white dragon", "blue eyes white dragon", "blue-eyes", "blue eyes", "kaiba's dragon", "burst stream of destruction"],
     "Blue-Eyes White Dragon", "Bird", ["Flies", "Breath", "ArticulatedTail"], 4.0, 2000, element="Electric", accent=CYAN,
     temperament="Aggressive", intensity=0.7, source="Yu-Gi-Oh!")
icon(["exodia", "exodia the forbidden one", "exodia obliterate"],
     "Exodia", "Kaiju", ["Beam", "Heavy", "Stomp"], 20.0, 50000, accent=GOLD, temperament="Lazy", intensity=0.3, source="Yu-Gi-Oh!")
icon(["megumin's explosion", "explosion magic", "megumin", "megumin explosion", "explosion spell"],
     "Megumin's Explosion", "Bomb", ["DropFromSky", "Explodes", "Heavy"], 6.0, 200, element="Fire", accent=RED, source="KonoSuba")
icon(["starburst stream", "kirito's dual swords", "dual blades kirito", "elucidator", "dark repulser", "kirito's swords", "dual wield kirito"],
     "Starburst Stream", "Sword", ["Blade", "Fast"], 1.2, 3, count=2, accent=CYAN, source="Sword Art Online")
icon(["spirit gun", "rei gun", "reigun", "yusuke's spirit gun", "shotgun spirit gun"],
     "Spirit Gun", "Cannon", ["Pierce", "Fast"], 0.8, 2, accent=CYAN, source="Yu Yu Hakusho")
icon(["rose whip", "kurama's rose whip", "rose whip kurama"],
     "Rose Whip", "Whip", ["Blade", "Pull"], 3.0, 1, element="Poison", accent=RED, source="Yu Yu Hakusho")
icon(["dragon of the darkness flame", "hiei's dragon", "darkness flame dragon", "kokuryuha"],
     "Dragon of the Darkness Flame", "Serpent", ["Flies", "Breath", "Homing"], 4.0, 300, element="Fire", accent=BLACK, source="Yu Yu Hakusho")
icon(["wave motion gun", "wave-motion gun", "hado ho", "yamato wave motion gun"],
     "Wave Motion Gun", "Cannon", ["Beam", "Heavy", "Explodes"], 4.0, 500, accent=CYAN, source="Space Battleship Yamato")
icon(["swordfish ii", "swordfish 2", "spike's ship", "swordfish mono racer"],
     "Swordfish II", "Vehicle", ["Flies", "Piloted", "Shoots"], 6.0, 3000, accent=RED, features=["Jet", "Cockpit"], source="Cowboy Bebop")
icon(["mach 5", "mach five", "mach go go go", "speed racer car"],
     "Mach 5", "Vehicle", ["Piloted", "Fast", "Blade"], 4.0, 900, accent=WHITE, features=["Wheels", "Saw"], source="Speed Racer")
icon(["cross punisher", "wolfwood's cross", "punisher cross", "wolfwood's gun"],
     "Cross Punisher", "Gun", ["Shoots", "Heavy", "Explodes"], 2.2, 60, accent=WHITE, source="Trigun")
icon(["kagune", "kaneki's kagune", "rinkaku", "kaneki kagune", "centipede kagune"],
     "Kagune", "Whip", ["Pierce", "Constrict", "Grab"], 3.5, 20, count=4, accent=RED, source="Tokyo Ghoul")
icon(["devil's footprints", "devils footprints", "shinra's feet", "shinra kusakabe feet", "fire force kicks"],
     "Devil's Footprints", "Jetpack", ["Thrust", "Fast"], 0.6, 2, element="Fire", accent=ORANGE, source="Fire Force")
icon(["mantis blades", "mantis blade", "edgerunners mantis blades", "arm blades cyberpunk"],
     "Mantis Blades", "Gauntlet", ["Blade", "Pierce", "Fast"], 1.2, 4, accent=RED, source="Cyberpunk: Edgerunners")
icon(["sandevistan", "sandy", "david's sandevistan"],
     "Sandevistan", "Cape", ["Fast"], 1.0, 2, accent=GREEN, source="Cyberpunk: Edgerunners")
icon(["death scythe", "soul eater scythe", "soul evans scythe", "maka's scythe"],
     "Death Scythe", "Axe", ["Blade", "Spin"], 2.4, 5, accent=RED, source="Soul Eater")
icon(["kurikara", "rin's sword", "blue exorcist sword", "blue flames sword"],
     "Kurikara", "Sword", ["Blade", "Breath"], 1.3, 3, element="Fire", accent=BLUE, source="Blue Exorcist")
icon(["dragon slayer roar", "fire dragon's roar", "natsu's roar", "fire dragon roar"],
     "Fire Dragon's Roar", "Staff", ["Breath", "Heavy"], 1.0, 2, element="Fire", accent=ORANGE, source="Fairy Tail")
icon(["gabimaru's ninpo", "ninpo fire", "gabimaru fire", "hollow gabimaru"],
     "Ninpo: Fire Method", "Gauntlet", ["Breath", "Fast"], 0.9, 3, element="Fire", accent=ORANGE, source="Hell's Paradise")
icon(["enten", "kagurabachi sword", "chihiro's sword", "enchanted blade enten", "tobimune", "shinuchi"],
     "Enten", "Sword", ["Blade", "Shoots"], 1.3, 3, element="Shadow", accent=ORANGE, source="Kagurabachi")
icon(["rudo's gloves", "gachiakuta gloves", "jinki gloves", "3r gloves"],
     "Rudo's Gloves", "Gauntlet", ["Grab", "Blunt"], 1.0, 4, accent=WHITE, source="Gachiakuta")


# ======================================================================================= emit
TINT_ORDER = ('kind', 'center', 'size', 'rot', 'tint', 'part', 'sub')


def f(v):
    v = round(float(v), 3)
    return repr(v) if v != int(v) else f"{int(v)}.0"


def vec(t):
    return "(" + ", ".join(f(x) for x in t) + ")"


def prim_ron(p):
    s = f"(kind: {p['kind']}, center: {vec(p['center'])}, size: {vec(p['size'])}"
    if any(abs(r) > 1e-9 for r in p['rot']):
        s += f", rot_deg: {vec(p['rot'])}"
    if p['tint'] != 'Primary':
        s += f", tint: {p['tint']}"
    if p['part']:
        s += f", part: {p['part']}"
    if p['sub']:
        s += ", subtract: true"
    return s + ")"


def recipe_ron(e, model, indent="        "):
    acc = f"Some({vec(e['accent'])})" if e.get('accent') else "None"
    mech = "[" + ", ".join(e['mech']) + "]"
    out = [f"(",
           f"{indent}    name: \"{e['display']}\",",
           f"{indent}    form: {e['form']},",
           f"{indent}    mechanics: {mech},",
           f"{indent}    element: {e.get('element', 'Light')},",
           f"{indent}    scale: {f(e['scale'])},",
           f"{indent}    mass: {f(e['mass'])},",
           f"{indent}    count: {e.get('count', 1)},",
           f"{indent}    accent: {acc},",
           f"{indent}    model: {model},"]
    if e.get('features'):
        out.append(f"{indent}    features: [{', '.join(e['features'])}],")
    if e.get('temperament'):
        out.append(f"{indent}    temperament: {e['temperament']},")
        out.append(f"{indent}    intensity: {f(e['intensity'] if e.get('intensity') is not None else 0.6)},")
    if e.get('payload'):
        p = e['payload']
        pe = dict(display=p['name'], form=p['form'], mech=p['mech'], element=p.get('element', 'Light'), scale=p['scale'],
                  mass=p['mass'], count=1, accent=None)
        seed = zlib.crc32(p['name'].encode()) & 0xffff
        out.append(f"{indent}    payload: Some({recipe_ron(pe, f'Procedural(seed: {seed})', indent + '    ')}),")
    out.append(f"{indent})")
    return "\n".join(out)


def entry_ron(e):
    names = ", ".join('"' + n + '"' for n in e['names'])
    if e['shape'] is not None:
        model = f"Icon(\"{e['id']}\")"
    else:
        model = f"Procedural(seed: {zlib.crc32(e['id'].encode()) & 0xffff})"
    lines = ["    IconEntry(", f"        names: [{names}],", f"        recipe: {recipe_ron(e, model)},"]
    if e['shape'] is not None:
        sp = e['shape'].spec()
        lines.append("        shape: Some((")
        lines.append("            prims: [")
        for p in sp['prims']:
            lines.append("                " + prim_ron(p) + ",")
        lines.append("            ],")
        if sp['parts']:
            lines.append("            parts: [")
            for q in sp['parts']:
                lines.append(f"                (id: {q['id']}, parent: {q['parent']}, pivot: {vec(q['pivot'])}, role: {q['role']}),")
            lines.append("            ],")
        lines.append("        )),")
    else:
        lines.append("        shape: None,")
    lines.append(f"        source: \"{e['source']}\",")
    lines.append("    ),")
    return "\n".join(lines)


BLOCK = {"bond", "sunny", "merry", "zeus", "infinity", "limitless", "aka", "ao", "kai", "hachi", "cleave", "furnace", "liberation",
         "boo", "supernova", "big bang", "godspeed", "bungee", "rumbling", "colossal", "armored", "the walls", "tusk", "sandy",
         "steamroller", "steel ball", "steel balls", "mimic", "ohm", "funnels", "dragoons", "valkyrie", "barbatos", "blue eyes",
         "blue-eyes", "cannon arm", "giant robot", "super robot", "instant teleport", "energy disc", "black fire", "pop off",
         "normal punch", "purgatory", "detroit", "hell flame", "robot soldier", "???%"}


DROP_IDS = {"kamehameha", "spirit_bomb", "voltron", "totoro"}   # already curated in assets/icons.ron (base wins)
BLOCK |= {"nimbus", "nine tailed fox", "nine-tailed fox", "mimic chest", "beyblade", "core drill", "great ape", "meteor shower",
          "nimbus cloud", "ninja star", "power hammer", "road roller", "shambles", "shuriken", "throwing stars"}


def main():
    E[:] = [e for e in E if e['id'] not in DROP_IDS]
    for e in E:
        e['names'] = [n for n in e['names'] if n not in BLOCK]
    # sanity: unique ids and names, lowercase names
    seen = {}
    ids = set()
    for e in E:
        assert e['id'] not in ids, e['id']
        ids.add(e['id'])
        for n in e['names']:
            assert n == n.lower() and n.strip(), n
            k = n.replace('-', ' ').replace("'", '')
            if k in seen and seen[k] != e['id']:
                print("DUP NAME", n, "in", e['id'], "and", seen[k])
            seen[k] = e['id']
    head = ("// Brightest Day - anime & manga icon pack (curated 2026-09). Schema: docs/ICONS_SCHEMA.md.\n"
            "// Names/titles only; recipes use the closed bd_core sets. Generated, then hand-checked.\n")
    body = "[\n" + "\n".join(entry_ron(e) for e in E) + "\n]\n"
    with open(OUT, 'w', encoding='utf-8', newline='\n') as fh:
        fh.write(head + body)
    prev = []
    for e in E:
        if e['shape'] is not None:
            prev.append(dict(id=e['id'], prims=e['shape'].spec()['prims'], accent=e.get('accent')))
    json.dump(prev, open(PREVIEW, 'w'))
    print("preview json:", PREVIEW)
    print(f"{len(E)} entries, {len(prev)} with shapes; prims max {max(len(p['prims']) for p in prev)}")


if __name__ == '__main__':
    main()
