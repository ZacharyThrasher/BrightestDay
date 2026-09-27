"""Generate assets/icons/marvel_names.txt: more Marvel/X-Men characters, teams and things for the
Wikidata/lexicon layers (`name | aliases | QID or blank | hint: form + mechanics`).

Source: tools/icons/marvel/wd_cache.json — Wikidata (CC0) items whose "from narrative universe" is
Earth-616, ordered by sitelinks, with English aliases/descriptions (fetched once; facts/names only).
Plus a small curated list of Marvel objects/vehicles/places below. Everything already named by any
icon pack, another *_names.txt or a power is skipped.

    python tools/icons/marvel/names_gen.py            # needs .known.tsv (see --known)
    python tools/icons/marvel/names_gen.py --known    # writes the single-word list for mvcheck, then run:
      MV_KNOWN_IN=... MV_KNOWN_OUT=tools/icons/marvel/.known.tsv cargo run --release \
        --manifest-path tools/icons/marvel/mvcheck/Cargo.toml
"""
import glob
import json
import os
import re
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.abspath(os.path.join(HERE, "..", "..", ".."))
MAX = 650

SKIP_TYPES = ("storyline", "series", "film", "issue", "comic book", "novel", "video game", "country", "city",
              "island", "prison", "company", "metal", "list", "Wikimedia", "imprint", "magazine", "story arc",
              "planet", "location", "universe", "dimension", "building")
KEEP_TYPES = ("character", "human", "mutant", "mutate", "extraterrestrial", "robot", "android", "deity", "demon",
              "vampire", "Asgardian", "Eternal", "Skrull", "Kree", "Inhuman", "Symbiote", "cyborg", "organization",
              "team", "group", "clone", "entity", "hybrid", "humanoid", "Shi'ar", "supersoldier", "persona",
              "artificial intelligence", "species", "set of comics characters", "hero name", "Homo mermanus")

# keyword -> (form override or None, mechanics, element)
RULES = [
    (r"\bdragon\b", ("Kaiju", ["Breath", "Flies"], "Fire")),
    (r"\b(planet|living planet)\b", ("Ball", ["Attract", "Heavy"], None)),
    (r"\b(insect|bee|ant|wasp|hornet|beetle|moth|firefly)\b", ("Insect", ["Flies", "Swarm"], None)),
    (r"\b(snake|serpent|cobra|python|viper)\b", ("Serpent", ["Constrict"], "Poison")),
    (r"\b(shark|fish|octopus|squid)\b", ("Fish", ["Grab"], "Water")),
    (r"\b(bird|hawk|falcon|owl|raven|crow|vulture|condor|eagle)\b", (None, ["Flies", "Glide"], None)),
    (r"\b(wolf|werewolf|lycanthrope)\b", (None, ["Blade", "Charge"], None)),
    (r"\b(tiger|panther|cat|lion|leopard|jaguar)\b", (None, ["Blade", "Fast"], None)),
    (r"\b(dog|hound)\b", ("Quadruped", ["Charge", "Grab"], None)),
    (r"\b(giant|titan|colossal|towering)\b", ("Kaiju", ["Heavy", "Stomp"], None)),
    (r"\b(robot|android|synthezoid|sentinel|cyborg|artificial intelligence|machine)\b", (None, ["Heavy", "Shoots"], None)),
    (r"\b(symbiote)\b", (None, ["Constrict", "Grab"], "Shadow")),
    (r"\b(vampire)\b", (None, ["Fast", "Grab"], "Shadow")),
    (r"\b(demon|hell|devil|demonic)\b", (None, ["Breath", "Grab"], "Fire")),
    (r"\b(fire|flame|pyro|burn|heat|inferno|sun|solar)\b", (None, ["Breath", "Shoots"], "Fire")),
    (r"\b(ice|frost|cold|cryo|snow|freez)", (None, ["Shoots"], "Ice")),
    (r"\b(electric|electricity|lightning|thunder|storm|shock)\b", (None, ["Beam", "Shoots"], "Electric")),
    (r"\b(telepath|telekine|psychic|psionic|mind|mental)", (None, ["Repel", "Shoots"], "Psychic")),
    (r"\b(poison|toxic|venom|acid)\b", (None, ["Shoots"], "Poison")),
    (r"\b(water|sea|ocean|aquatic|atlantean|hydro)\b", (None, ["Grab", "Shoots"], "Water")),
    (r"\b(radiation|radioactive|gamma|nuclear|cosmic|energy)\b", (None, ["Beam", "Shoots"], "Atomic")),
    (r"\b(wind|air|tornado)\b", (None, ["Flies", "Repel"], "Wind")),
    (r"\b(earth|rock|stone|sand|mud|lava)\b", (None, ["Blunt", "Heavy"], "Earth")),
    (r"\b(darkness|shadow|dark)\b", (None, ["Grab"], "Shadow")),
    (r"\b(magnet|magnetic)", (None, ["Attract", "Repel"], None)),
    (r"\b(teleport|portal|dimension)", (None, ["Portal"], None)),
    (r"\b(stretch|elastic|rubber|malleable)", (None, ["Bounce", "Grab"], None)),
    (r"\b(shrink|size)", (None, ["Shrink"], None)),
    (r"\b(grow)", (None, ["Grow"], None)),
    (r"\b(speed|speedster|fast|swift|quick)\b", (None, ["Charge", "Fast"], None)),
    (r"\b(archer|arrow|bow)\b", (None, ["Pierce", "Shoots"], None)),
    (r"\b(sword|swordsman|blade|ninja|samurai|assassin|claw|claws|katana)\b", (None, ["Blade", "Fast"], None)),
    (r"\b(gun|guns|sniper|marksman|mercenary|soldier|gunman|agent|spy)\b", (None, ["Shoots"], None)),
    (r"\b(armor|armour|armored|armoured|shield)\b", (None, ["Guard", "Shoots"], None)),
    (r"\b(fly|flying|flight|wings|winged|angel)\b", (None, ["Flies"], None)),
    (r"\b(spider|web)\b", (None, ["Pull", "Sticky"], None)),
    (r"\b(strength|strong|brute|strongman|hulk|powerhouse)\b", (None, ["Blunt", "Heavy"], None)),
    (r"\b(god|goddess|deity|asgardian|eternal|celestial|cosmic entity|abstract entity)\b", (None, ["Beam", "Flies"], None)),
    (r"\b(alien|extraterrestrial|kree|skrull|shi'ar)\b", (None, ["Shoots"], None)),
    (r"\b(sorcer|magic|mystic|witch|wizard|occult)", (None, ["Homing", "Shoots"], "Psychic")),
    (r"\b(monster|beast|creature)\b", (None, ["Charge", "Grab"], None)),
]

# Curated objects / vehicles / places / lesser characters not covered by Earth-616 characters.
CURATED = [
    ("captain america's first shield", "heater shield cap", "", "Shield + Guard, Ricochet"),
    ("uru", "uru metal; asgardian metal", "", "Boulder + Heavy, Electric"),
    ("adamantium shield", "", "", "Shield + Guard, Heavy"),
    ("carbonadium", "carbonadium coil", "", "Whip + Constrict, Link"),
    ("ultimate nullifier device", "", "", "Gun + Beam, Pierce"),
    ("nega-bands", "negabands; quantum bands", "", "Gauntlet + Beam, Atomic"),
    ("quantum bands", "", "", "Gauntlet + Beam, Atomic"),
    ("tesseract energy", "", "", "Ball + Portal, Explodes"),
    ("infinity gems", "soul gems", "", "Ball + Homing, Psychic"),
    ("the aether reality stone", "", "", "Ball + Sticky, Shadow"),
    ("m'kraan crystal", "mkraan crystal", "", "Pillar + Portal, Beam"),
    ("the cosmic control rod", "", "", "Staff + Beam, Repel"),
    ("the serpent crown", "", "", "Dome + Barrier, Psychic"),
    ("the hand of doom", "", "", "Gauntlet + Grab, Electric"),
    ("doombot", "doombots; doom bots", "", "Biped + Shoots, Swarm"),
    ("life model decoy", "lmd", "", "Biped + Shoots, Swarm"),
    ("spider-slayer", "spider slayer; spider-slayers", "", "Arachnid + Grab, Shoots"),
    ("goblin bombs", "", "", "Bomb + Explodes, Bounce"),
    ("razor bats", "goblin razor bats", "", "Disc + Blade, Homing"),
    ("web glider", "web cape", "", "Wings + Glide"),
    ("spider-cycle", "spider cycle", "", "Vehicle + Charge, Piloted"),
    ("spider-armor", "spider armor mk iv", "", "Biped + Guard, Piloted"),
    ("iron spider arms", "waldo arms", "", "Arachnid + Grab, Pierce"),
    ("hulkbuster armor", "hulkbuster suit", "", "Biped + Heavy, Piloted"),
    ("iron patriot", "iron patriot armor", "", "Biped + Flies, Piloted, Shoots"),
    ("rescue armor", "pepper potts armor", "", "Biped + Flies, Guard, Piloted"),
    ("mark 42", "mark xlii", "", "Biped + Flies, Piloted"),
    ("unibeam", "iron man unibeam", "", "Gauntlet + Beam"),
    ("arc reactor", "stark arc reactor", "", "Ball + Explodes, Atomic"),
    ("stark tower", "avengers tower", "", "Pillar + Heavy"),
    ("baxter building", "", "", "Pillar + Heavy"),
    ("the raft", "raft prison", "", "Cage + Trap"),
    ("x-copter", "x copter", "", "Vehicle + Flies, Piloted"),
    ("velocipede", "x-men velocipede", "", "Vehicle + Fast, Piloted"),
    ("fantasticar mk2", "fantasti-car", "", "Vehicle + Flies, Piloted"),
    ("fantastic four pogo plane", "pogo plane", "", "Vehicle + Flies, Piloted"),
    ("the ghost rider bike", "hellcycle mk2", "", "Vehicle + Charge, Fire, Piloted"),
    ("blackbird sr-71", "x-men sr-71", "", "Vehicle + Flies, Piloted, Shoots"),
    ("avengers quinjet mk2", "", "", "Vehicle + Flies, Piloted, Shoots"),
    ("shield flying car", "s.h.i.e.l.d. flying car", "", "Vehicle + Flies, Piloted"),
    ("sanctum sanctorum", "", "", "Dome + Barrier, Portal"),
    ("wand of watoomb", "", "", "Staff + Beam, Psychic"),
    ("book of the vishanti", "", "", "Staff + Barrier, Psychic"),
    ("orb of agamotto", "", "", "Ball + Homing, Psychic"),
    ("the darkforce", "darkforce dimension", "", "Dome + Portal, Shadow"),
    ("hellfire club", "the hellfire club", "", "Biped + Swarm, Fire"),
    ("the hand ninjas", "the hand", "", "Biped + Blade, Swarm"),
    ("aim beekeepers", "a.i.m. agents", "", "Biped + Shoots, Swarm"),
    ("mandroids", "shield mandroid", "", "Biped + Heavy, Shoots"),
    ("kree sentry", "kree sentry robot", "", "Kaiju + Beam, Stomp"),
    ("celestial head", "knowhere", "", "Platform + Heavy"),
    ("moon knight's truncheon", "moon knight staff", "", "Staff + Blunt"),
    ("cable's psimitar", "psimitar", "", "Sword + Blade, Psychic"),
    ("wolverine's katana", "muramasa blade", "", "Sword + Blade"),
    ("x-force claws", "", "", "Gauntlet + Blade, Pierce"),
    ("ms. marvel embiggen", "embiggen", "", "Gauntlet + Grow, Blunt"),
    ("squirrel army", "squirrels", "", "Quadruped + Swarm, Fast"),
    ("frog thor", "throg's hammer", "", "Hammer + Returning, Electric"),
    ("mjolnir 2099", "", "", "Hammer + Returning, Electric"),
    ("storm's staff", "", "", "Staff + Electric, Beam"),
    ("black bolt's tuning fork", "", "", "Staff + Repel"),
]


from names_hints import HINTS  # noqa: E402  hand-written hints for notable characters


def taken():
    out = set()
    files = [os.path.join(ROOT, "assets", "icons.ron")] + glob.glob(os.path.join(ROOT, "assets", "icons", "*.ron"))
    for fp in files:
        t = open(fp, encoding="utf-8").read()
        for m in re.finditer(r"names:\s*\[(.*?)\]", t, re.S):
            out.update(n.lower() for n in re.findall(r'"([^"]*)"', m.group(1)))
    for fp in glob.glob(os.path.join(ROOT, "assets", "icons", "*_names.txt")):
        if fp.endswith("marvel_names.txt"):
            continue
        for line in open(fp, encoding="utf-8"):
            parts = [p.strip() for p in line.split("|")]
            if len(parts) >= 2:
                out.add(parts[0].lower())
                out.update(a.strip().lower() for a in parts[1].split(";") if a.strip())
    pdir = os.path.join(ROOT, "assets", "powers")
    for fp in [os.path.join(pdir, f) for f in sorted(os.listdir(pdir)) if f.endswith(".ron")]:
        t = open(fp, encoding="utf-8").read()
        for m in re.finditer(r"names:\s*\[(.*?)\]", t, re.S):
            out.update(n.lower() for n in re.findall(r'"([^"]*)"', m.group(1)))
    return out


def clean(s):
    s = s.lower().strip()
    s = re.sub(r"\s*\((marvel comics|character|comics|marvel|marvel cinematic universe|earth-616)\)", "", s)
    s = s.replace("|", " ").replace(";", " ").replace('"', "").replace(",", "")
    return re.sub(r"\s+", " ", s).strip()


def hint_for(e):
    text = " ".join([e["label"], e["desc"], e["types"]]).lower()
    typ = e["types"]
    form = "Biped"
    mech, el = [], None
    if any(k in typ for k in ("organization", "team", "group", "set of comics")):
        mech.append("Swarm")
    for pat, (f, ms, elem) in RULES:
        if re.search(pat, text):
            if f and form == "Biped":
                form = f
            for m in ms:
                if m not in mech and len(mech) < 3:
                    mech.append(m)
            if elem and not el:
                el = elem
        if len(mech) >= 3:
            break
    if not mech:
        mech = ["Blunt"]
    return "%s + %s" % (form, ", ".join(mech + ([el] if el else [])))


def main():
    cache = json.load(open(os.path.join(HERE, "wd_cache.json"), encoding="utf-8"))
    tk = taken()
    known_path = os.path.join(HERE, ".known.tsv")
    if "--known" in sys.argv:
        words = sorted({clean(e["label"]) for e in cache if " " not in clean(e["label"])})
        open(os.path.join(HERE, ".known_in.txt"), "w", encoding="utf-8").write("\n".join(words) + "\n")
        print("wrote %d words -> .known_in.txt" % len(words))
        return
    known = {}
    if os.path.exists(known_path):
        for line in open(known_path, encoding="utf-8"):
            p = line.rstrip("\n").split("\t")
            if len(p) >= 2:
                known[p[0]] = p[1] == "1"
    lines, used = [], set()

    def add(name, aliases, qid, hint):
        name = clean(name)
        if not name or name in tk or name in used or name.startswith("q") and name[1:].isdigit():
            return False
        al = []
        for a in aliases:
            a = clean(a)
            if a and a != name and a not in tk and a not in used and a not in al and len(a) <= 40 and not re.search(r"[^\x20-\x7e]", a):
                al.append(a)
        used.add(name)
        used.update(al)
        lines.append("%s | %s | %s | hint: %s" % (name, "; ".join(al[:4]), qid, HINTS.get(name.replace(" marvel", ""), hint)))
        return True

    for e in cache:
        if len(lines) >= MAX - len(CURATED):
            break
        typ = e["types"]
        if any(s.lower() in typ.lower() for s in SKIP_TYPES) and not any(k.lower() in typ.lower() for k in KEEP_TYPES):
            continue
        if not typ or not any(k.lower() in typ.lower() for k in KEEP_TYPES):
            continue
        label = clean(e["label"])
        if label in tk or "the " + label in tk or label.replace("-", " ") in tk:
            continue  # an icon / power / other names file already covers this character
        if re.search(r"[^\x20-\x7e]", label) or len(label) > 40:
            continue
        aliases = list(e["aliases"])
        name = label
        if " " not in label:
            # a word the lexicon already knows (or unchecked): qualify it, keep the bare word as an alias
            name = "%s marvel" % label
            aliases = [label] + aliases
        add(name, aliases, e["qid"], hint_for(e))
    for n, a, q, h in CURATED:
        add(n, [x for x in a.split(";")], q, h)
    import mvlib
    ok = mvlib.FORMS | mvlib.MECHS | mvlib.ELEMS
    for ln in lines:
        h = ln.rsplit("hint:", 1)[1].strip()
        form, _, rest = h.partition(" + ")
        words = [form] + [w.strip() for w in rest.split(",") if w.strip()]
        assert form in mvlib.FORMS and all(w in ok for w in words), ln
    out = os.path.join(ROOT, "assets", "icons", "marvel_names.txt")
    open(out, "w", encoding="utf-8", newline="\n").write("\n".join(lines) + "\n")
    print("wrote %s: %d lines" % (out, len(lines)))


if __name__ == "__main__":
    main()
