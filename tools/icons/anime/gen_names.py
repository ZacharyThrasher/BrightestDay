"""Clean + write assets/icons/anime_names.txt from names*.txt blocks."""
import re, os, glob
HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.environ.get("BD_ROOT") or os.path.abspath(os.path.join(HERE, "..", "..", ".."))
OUT = os.path.join(ROOT, "assets", "icons", "anime_names.txt")
FORMS = set("Hammer Sword Axe Spear Shield Gauntlet Whip Gun Bow Cannon Staff Disc Wings Parachute Jetpack Cape Biped Kaiju Quadruped Serpent Bird Fish Arachnid Insect Blob Wall Ramp Pillar Dome Cage Platform Trampoline Ball Anvil Bomb Boulder Rocket Vehicle".split())
MECHS = set("Blunt Blade Pierce Guard Returning Ricochet Shoots Beam Explodes Grab Pull SlowFall Glide Thrust Bounce Launch Barrier Trap Homing ArticulatedTail Flies Charge Stomp Breath Swarm Spin DropFromSky Sticky Fast Heavy Constrict Walks Piloted Portal Attract Repel Shrink Grow Link".split())
ELEMS = set("Light Fire Electric Ice Water Wind Earth Poison Psychic Shadow Atomic".split())
FEATS = set("Wheels Treads Pistons Gears Boiler Smokestack Pipes Exhaust Propeller Rotor Jet Turbine Sails Mast Cockpit Cab Windows Turret Barrels Drill Saw CraneArm Chains Antenna Dish Headlight Plow Fins Spikes Armor Bell Cage".split())
TEMPS = set("Calm Aggressive Fearful Protective Playful Lazy Chaotic Sneaky".split())

RENAME = {
    "cell": "perfect cell", "trunks": "future trunks", "piccolo": "piccolo namekian", "cooler": "lord cooler", "hit": "hit assassin",
    "power": "power blood fiend", "iron": "iron shadow soldier", "greed": "greed shadow soldier", "en": "en dorohedoro",
    "happy": "happy exceed", "king": "king one punch man", "bang": "silver fang bang", "darkness": "darkness konosuba",
    "father": "father homunculus", "pride": "pride homunculus", "lust": "lust homunculus", "gluttony": "gluttony homunculus",
    "envy": "envy homunculus", "scar": "scar fma", "guts": "guts berserk", "ram": "ram re zero", "mob": "mob psycho",
    "san": "san mononoke", "howl": "howl pendragon", "fern": "fern frieren", "stark": "stark frieren", "land": "land frieren",
    "hollow": "bleach hollow", "chad": "yasutora sado", "luna": "luna cat", "artemis": "artemis cat", "puck": "puck berserk",
    "priestess": "priestess goblin slayer", "black star": "black star soul eater", "kiwi": "kiwi edgerunners",
    "maine": "maine edgerunners", "sylph": "sylph black clover", "salamander": "salamander black clover",
    "undine": "undine black clover", "gnome": "gnome black clover", "grimoire": "black clover grimoire",
    "infernal": "infernal fire force", "dimple": "dimple mob psycho", "metal bat": "metal bat hero",
    "harvest": "harvest stand", "aerosmith": "aerosmith stand", "sex pistols": "sex pistols stand", "the hand": "the hand stand",
    "made in heaven": "made in heaven stand", "king crimson": "king crimson stand", "killer queen": "killer queen stand",
    "betelgeuse": "petelgeuse romanee-conti", "panda": "panda jujutsu kaisen", "hanami": "hanami curse", "dagon": "dagon curse",
    "great serpent": "great serpent shikigami", "dragon bone": "maki's dragon bone", "brook": "soul king brook",
    "buggy": "buggy the clown", "crocodile": "sir crocodile", "yamato": "yamato one piece", "red hawk": "luffy red hawk",
    "endeavor": "endeavor hero", "hawks": "hawks hero", "eraser head": "shota aizawa", "stain": "hero killer stain",
    "overhaul": "overhaul chisaki", "gran torino": "gran torino hero", "knuckleduster": "knuckleduster vigilante",
    "nobunaga": "nobunaga hazama", "franklin": "franklin bordeau", "razor": "razor hxh", "lilith": "lilith evangelion",
    "israfel": "israfel evangelion", "gm": "gm mobile suit", "dom": "dom mobile suit", "forest spirit": "forest spirit mononoke",
    "parakeets": "parakeet army ghibli", "space sword": "uranus space sword", "misty": "misty pokemon", "brock": "brock pokemon",
    "roy": "roy pokemon horizons", "evil eye": "evil eye dandadan", "antares": "antares dragon monarch",
    "slur": "slur sakamoto days", "ebisu": "ebisu dorohedoro", "ymir": "ymir fritz", "jiji": "jiji cat",
    "arrietty": "arrietty borrower", "kaguya otsutsuki": "kaguya otsutsuki", "rebecca": "rebecca edgerunners", "kon": "kon bleach",
}
DROP = {"dragon ball goku's hat", "chainsaw man cosplay", "kizaru light", "yoru", "solar flare", "smoke bomb", "straw hat",
        "flare gun", "wisteria", "meowth", "terapagos", "rollout", "magic pen", "high orc", "ice elf", "philosopher's stone",
        "vegeta's galick", "kiki flying", "aura sphere", "razor leaf", "shadow ball", "string shot",
        "vine whip", "kodama", "ponyo", "flatwoods monster"}
BAN_ALIAS = {"baby", "lee", "rotation", "art is an explosion", "pain", "kaguya", "eight gates", "chopper", "robin", "ace", "law",
             "birdcage", "ice age", "seraphim", "kuma", "wolves", "the almighty", "gillian", "strongest sorcerer", "todo", "jackpot",
             "comedian", "dead calm", "infinity train", "typhoon", "eternity", "bat", "leech", "darkness", "future", "tomato",
             "zombies", "falling", "asa", "jean", "connie", "annie", "porco", "abnormal", "smiling", "cart", "zero gravity",
             "hardening", "momo", "pinky", "acid", "tape", "decay", "blue flames", "deep purple", "needles", "spell cards", "rei",
             "plug", "char", "jim", "trojan horse", "simon", "lancelot", "lio", "arbalest", "juggernaut", "sophie", "scarecrow",
             "sen", "incubator", "cerberus", "fate", "bardiche", "ash", "pay day", "water shuriken", "mantis shrimp", "kasaka",
             "twilight", "yuri", "nightfall", "lu", "caiman", "aqua", "ruby", "rin", "coco", "lucy", "rebecca", "smasher", "blizzard",
             "joseph", "jonathan", "gyro", "johnny", "ed", "wrench", "armstrong", "wrath", "bradley", "hawkeye", "spike", "the major",
             "zanbato", "gray", "time machine", "furoshiki", "table cloth", "jackal", "harkonnen", "witch hunter", "centipede", "kid",
             "excalibur", "zero", "light", "kira", "smith", "albedo", "momonga", "atomic", "steal", "goliath", "behemoth", "sirius",
             "kon", "ai", "sae", "l", "the baron", "haro", "boo", "aura", "chrome", "mallet", "lapis", "lazuli", "prometheus",
             "hancock", "borsalino", "resonance", "permeation", "knuckle", "gin", "hollows", "infernals", "suzaku", "yami", "seiko", "dmg", "alucard", "falco",
             "rotom dex", "zantetsuken", "shishigami", "cursed sword", "obelisk", "yasakani no magatama"}


def main():
    icon_names = set()
    for f in glob.glob(os.path.join(ROOT, "assets", "icons.ron")) + glob.glob(os.path.join(ROOT, "assets", "icons", "*.ron")):
        s = open(f, encoding='utf-8').read()
        for m in re.finditer(r'names:\s*\[(.*?)\]', s, re.S):
            icon_names.update(re.findall(r'"([^"]*)"', m.group(1)))
    key = lambda n: re.sub(r"[^a-z0-9 ]", "", n.lower().replace('-', ' ')).strip()
    icon_keys = {key(n) for n in icon_names}
    lines, seen, n_ok, dropped = [], set(), 0, []
    lines.append("# Brightest Day - anime & manga Tier-A names (2026-09). name | aliases | QID | hint (Form/Mechanic/Element/Feature/Temperament + base word).")
    lines.append("# Recent/popular series: characters, summons, creatures, weapons, techniques. Names/titles only.")
    for src in sorted(glob.glob(os.path.join(HERE, "names*.txt"))):
        for raw in open(src, encoding='utf-8'):
            raw = raw.rstrip("\n")
            if not raw.strip():
                continue
            if raw.startswith("#"):
                lines.append(raw)
                continue
            parts = [p.strip() for p in raw.split("|")]
            assert len(parts) == 3, raw
            name, aliases, hint = parts
            name = name.lower()
            if name in DROP:
                continue
            name = RENAME.get(name, name)
            als = [a.strip().lower() for a in aliases.split(";") if a.strip()]
            als = [a for a in als if a not in BAN_ALIAS and a != name and key(a) not in icon_keys and key(a) not in seen]
            if key(name) in icon_keys or key(name) in seen:
                if als:
                    name, als = als[0], als[1:]
                else:
                    dropped.append(name)
                    continue
            # one Form per hint; validate tokens
            toks, form_seen = [], False
            for t in hint.split():
                if t in FORMS:
                    if form_seen:
                        continue
                    form_seen = True
                toks.append(t)
            for a in [name] + als:
                seen.add(key(a))
            lines.append(f"{name} | {'; '.join(als)} |  | {' '.join(toks)}")
            n_ok += 1
    with open(OUT, 'w', encoding='utf-8', newline='\n') as fh:
        fh.write("\n".join(lines) + "\n")
    print(n_ok, "names written; dropped as duplicates:", dropped)


if __name__ == '__main__':
    main()
