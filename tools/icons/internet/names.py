import os, re, sys
from pathlib import Path
sys.argv = ["gen.py", os.devnull]
sys.path.insert(0, str(Path(__file__).resolve().parent))
import gen  # builds ENTRIES (writes nothing)
ROOT = gen.ROOT
from lib import ENTRIES, MECH_ORDER, FEAT_ORDER, FORMS, ELEMENTS, TEMPS

DROP = set("""hawk tuah|gyatt|edging|sad pablo|drake hotline|jean phil|lana's alligator|misery girl|star by face|rizzler|brat summer|demure|fanum tax|skibidi rizz|soyjak|yes chad|raccoon|possum|opossum|pigeon|seagull|shrimp|crab|corgi|pug|axolotl|quokka|blobfish|trash panda|mogging|goblin mode|touch grass|ohio|mewing|sigma face|aura""".split("|"))
ron_names = set(n for e in ENTRIES for n in e[0]) | gen.OTHER
out, seen, bad = [], set(), []
for line in open(Path(__file__).resolve().parent / "names_src.txt", encoding="utf-8"):
    line = line.strip()
    if not line or line.startswith("#"):
        continue
    name, aliases, hint = [x.strip() for x in line.split("|")]
    name = name.lower()
    if name in DROP or name in ron_names or name in seen:
        continue
    al = [a.strip().lower() for a in aliases.split(";") if a.strip()]
    al = [a for a in al if a not in ron_names and a not in seen and a != name]
    m = re.match(r"^(\w+)\s*\+\s*([^()]*?)\s*(?:\((\w+)\))?$", hint)
    if not m:
        bad.append(line); continue
    form, toks, el = m.group(1), [t.strip() for t in m.group(2).split(",") if t.strip()], m.group(3)
    if form not in FORMS:
        bad.append(line); continue
    mech, feat, temp = [], [], None
    for t in toks:
        if t in MECH_ORDER: mech.append(t)
        elif t in FEAT_ORDER: feat.append(t)
        elif t in TEMPS: temp = t
        elif t in ELEMENTS: el = el or t
        elif t == "Whip": mech.append("Pull")
        else: bad.append(f"{line}  <{t}>")
    parts = [form + (" + " + ", ".join(mech) if mech else "")]
    if feat: parts.append("features " + ", ".join(feat))
    if el and el != "Light": parts.append("element " + el)
    if temp: parts.append("temperament " + temp)
    seen.add(name); seen.update(al)
    out.append(f"{name} | {'; '.join(al)} |  | hint: {'; '.join(parts)}")
hdr = ["# Brightest Day tier-A names: internet culture, memes, toys, sports, everyday objects, mythology & folklore,",
       "# cryptids, legendary artifacts, D&D, tech mascots. Format: name | aliases | QID or blank | hint: form + mechanics",
       "# (QIDs left blank rather than guessed; resolve via Wikidata label search at build time.)"]
open(ROOT / "assets/icons/internet_names.txt", "w", encoding="utf-8").write("\n".join(hdr + out) + "\n")
print(len(out), "names;", len(bad), "problems")
for b in bad: print("  ", b)
