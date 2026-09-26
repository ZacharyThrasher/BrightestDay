import glob, json, re, sys, time, urllib.parse, urllib.request, os
from ftlib import FORMS, MECHS, FEATS, TEMPS, ELEMS, ROOT, taken_names
from names_data import DATA

ICONS = open(os.path.join(ROOT, "assets", "icons", "film_tv.ron"), encoding="utf-8").read()
icon_names = set()
for m in re.finditer(r"names: \[(.*?)\]", ICONS):
    icon_names.update(re.findall(r'"(.*?)"', m.group(1)))
# Names owned elsewhere (base icons.ron, other packs' icons and tier-A name lists) are left to their owners.
icon_names |= set(taken_names())
for fp in glob.glob(os.path.join(ROOT, "assets", "icons", "*_names.txt")):
    if os.path.basename(fp) == "film_tv_names.txt":
        continue
    for line in open(fp, encoding="utf-8"):
        cols = [c.strip() for c in line.split("|")]
        if len(cols) >= 2 and not line.startswith("#"):
            icon_names.add(cols[0].lower())
            icon_names.update(a.strip().lower() for a in cols[1].split(";") if a.strip())

QUAL = {"Marvel": "marvel", "DC": "dc", "Star Wars": "star wars", "Lord of the Rings": "lotr", "Harry Potter": "harry potter",
        "Monsterverse": "kaiju", "Jurassic": "jurassic", "Pacific Rim": "pacific rim", "Alien": "alien",
        "Stranger Things": "stranger things", "Arcane": "arcane", "Animation — Pixar": "pixar",
        "Animation — Disney": "disney", "Animation — DreamWorks": "dreamworks", "Classic cartoons": "cartoon",
        "Transformers": "transformers", "Ghostbusters": "sci-fi", "Horror": "horror", "Live action": "movie",
        "Power Rangers": "tv"}
AMBIG = set("""blade vision storm mantis nebula ghost beast rogue sandman lizard rhino scorpion vulture spot prowler toxin
leader falcon robin bane raven cheetah atom parasite ares trench topo titan conquest zoey mira rumi rex hamm joy anger
sadness anxiety bingo muffin genie scar ralph animal sentry orb aether domino cable juggernaut apocalypse mystique
colossus iceman dash boo bruno luisa pua pascal maximus sven kai po tigress fiona mongo dino dug remy luca bullseye sox
jessie forky woody rosie naga momo arlo elio ember dante pepita horton roach striga wights colt salem teddy knuckles
shadow jazz arcee ratchet ironhide mirage wheeljack scourge sweeps energon chica bonnie foxy critters stranger adar
halbrand engineer thia dek deacon hyde q unity gearhead gary skeets relic butcher ophidian adara proselyte krona
salaak smile weasel vigilante hush firefly ventriloquist nightwing batgirl alfred beorn gollum eve kevin appa
frozone boo lumiere bruni spirit hodor max""".split())
DROP = {"pegasus", "medusa", "kraken", "hydra", "minotaur", "harpies", "chimera", "talos", "zombies", "piranha",
        "anaconda", "leprechaun", "the birds", "zombie horde", "excalibur", "hercules' club", "bird box", "smile entity"}
GENERIC_ALIASES = {"werewolf", "tornado", "black hole", "chainsaw", "frying pan", "grappling hook", "slug", "stampede",
                   "mine cart", "totem", "harvester", "net gun", "exosuit", "power armor", "comet", "rocket", "cap",
                   "gadget", "direwolf", "giant ants", "martians", "nova", "void", "glaive", "leviathan", "phoenix",
                   "doom", "oa", "synthetic", "chip", "brave", "encanto", "lasso", "slender man", "clover",
                   "trinity", "the voice", "the ark", "inferno", "stripe", "megan", "chupacabra", "starfish",
                   "mini cooper", "falling piano", "giant magnet", "ice dragon", "crysknife", "nos", "tomcat",
                   "the one matrix", "salem", "ghost", "the gate", "spitter", "compies", "explorer", "morpher",
                   "silver sword", "pixies", "abu", "the stranger", "red balloon", "puzzle box", "boomstick",
                   "the thing monster", "killer car", "stranger", "herbie"}
# Ceded to the TV/movie pack (assets/icons/shows.ron has hand-authored icons for these; lead decision 2026-09-26).
# Dropped as row names and as aliases so a regeneration never takes them back.
CEDED = {"homer simpson", "bart simpson", "rick's spaceship", "mr burns' hounds", "mr. burns' hounds", "drogon", "viserion", "ghost the direwolf",
         "iron throne", "freddy krueger", "freddy krueger's glove", "jason voorhees", "michael myers", "chucky", "ghostface",
         "pennywise", "pennywise balloon", "leatherface", "weapons aunt gladys", "annabelle", "candyman's hook", "krites",
         "m3gan", "john wick", "fast and furious charger", "dom's charger", "neo", "nitrous boost", "sheriff woody",
         "toy story rex", "aang's staff", "zippleback", "holtzman shield", "dune crysknife", "baron harkonnen",
         "the baron harkonnen", "spice harvester", "carryall", "paul atreides", "muad'dib", "sardaukar"}
BANNED_ALIASES = {"why did it have to be snakes", "beam me", "it's so fluffy", "release the kraken"}

rows, seen, errs = [], set(), []
section = ""
for line in DATA.strip().splitlines():
    line = line.strip()
    if not line:
        continue
    if line.startswith("##"):
        section = line[2:].strip()
        continue
    if section == "Transformers":  # owned by transformers_names.txt
        continue
    parts = [p.strip() for p in line.split("|")]
    name, aliases, hint = parts[0], parts[1], parts[2]
    title = parts[3][1:].strip() if len(parts) > 3 and parts[3].startswith("@") else None
    form, _, rest = hint.partition("+")
    form = form.strip()
    toks = [t.strip() for t in rest.split(",") if t.strip()]
    if form not in FORMS:
        errs.append(("form", name, form))
    bad = [t for t in toks if t not in MECHS | FEATS | TEMPS | ELEMS]
    if bad:
        errs.append(("tok", name, bad))
    key = name.lower()
    if key in DROP:
        continue
    als = [a.strip().lower() for a in aliases.split(";") if a.strip()]
    if key in AMBIG:
        q = next((v for k, v in QUAL.items() if section.startswith(k)), "movie")
        als = [key + " " + q] + als
        key = q + " " + key
        name = key
    als = [a for a in als if a not in BANNED_ALIASES and a not in GENERIC_ALIASES and a not in CEDED and a != key]
    if key in icon_names or key in seen or key in CEDED:
        continue
    als = [a for a in als if a not in icon_names and a not in seen]
    seen.add(key)
    seen.update(als)
    rows.append(dict(name=name, aliases=als, title=title, form=form, toks=toks, section=section))

if errs:
    for e in errs:
        print("ERR", e)
    sys.exit(1)

# ---- Wikidata QIDs (only for rows with an explicit enwiki title; accepted only if the description fits)
cache_p = os.path.join(os.path.dirname(os.path.abspath(__file__)), "qid_cache.json")
cache = json.load(open(cache_p)) if os.path.exists(cache_p) else {}
titles = [r["title"] for r in rows if r["title"] and r["title"] not in cache]
UA = "BrightestDayIconBuild/0.1 (offline content tool)"
for i in range(0, len(titles), 50):
    batch = titles[i:i + 50]
    q = urllib.parse.urlencode({"action": "wbgetentities", "sites": "enwiki", "titles": "|".join(batch),
                                "props": "descriptions|sitelinks", "sitefilter": "enwiki", "languages": "en",
                                "format": "json", "redirects": "yes"})
    req = urllib.request.Request("https://www.wikidata.org/w/api.php?" + q, headers={"User-Agent": UA})
    data = json.load(urllib.request.urlopen(req, timeout=60))
    got = {}
    for qid, ent in data.get("entities", {}).items():
        if not qid.startswith("Q"):
            continue
        t = ent.get("sitelinks", {}).get("enwiki", {}).get("title")
        d = ent.get("descriptions", {}).get("en", {}).get("value", "")
        got[t] = (qid, d)
    for t in batch:
        cache[t] = got.get(t)  # redirected titles miss here (kept blank; safer than guessing)
    time.sleep(0.5)
json.dump(cache, open(cache_p, "w"), indent=0)

FICTION = re.compile(r"fictional|character|comic|film|television|series|superhero|supervillain|villain|kaiju|"
                     r"monster|creature|robot|spacecraft|starship|device|weapon|dragon|franchise|toy|animated|"
                     r"mascot|marvel|dc |disney|pixar|anime|novel|legendary|mytholog|vehicle|car |doll|species",
                     re.I)
out = []
qids = 0
sec = None
for r in rows:
    if r["section"] != sec:
        sec = r["section"]
        pass
    qid = ""
    if r["title"] and cache.get(r["title"]):
        q, d = cache[r["title"]]
        if FICTION.search(d) and "list" not in d.lower() and "disambiguation" not in d.lower():
            qid = q
            qids += 1
    hint = r["form"] + (" + " + ", ".join(r["toks"]) if r["toks"] else "")
    out.append("%s | %s | %s | hint: %s" % (r["name"].lower(), "; ".join(r["aliases"]), qid, hint))
open(os.path.join(ROOT, "assets", "icons", "film_tv_names.txt"), "w", encoding="utf-8", newline="\n").write(
    "\n".join(out) + "\n")
print(len(rows), "names,", qids, "with QIDs")
