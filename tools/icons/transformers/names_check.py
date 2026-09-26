"""Check assets/icons/transformers_names.txt: format, enum words in hints, duplicates within the file and
against every other icon name (all assets/icons/*.ron + *_names.txt), and list single-word names for review.

    python tools/icons/transformers/names_check.py [path]
"""
import glob
import os
import re
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
ROOT = os.path.abspath(os.path.join(HERE, "..", "..", ".."))
import tflib  # noqa: E402

ENUMS = set(tflib.MECH_ORDER) | set(tflib.FEAT_ORDER) | tflib.FORMS | tflib.TEMPS | tflib.ELEMS


def norm(s):
    return re.sub(r"\s+", " ", s.strip().lower())


def ron_names(path):
    out = set()
    for m in re.finditer(r"names:\s*\[(.*?)\]", open(path, encoding="utf-8").read(), re.S):
        out |= {norm(x) for x in re.findall(r'"((?:[^"\\]|\\.)*)"', m.group(1))}
    return out


def txt_names(path):
    out = set()
    for ln in open(path, encoding="utf-8"):
        if not ln.strip() or ln.startswith("#"):
            continue
        p = [x.strip() for x in ln.split("|")]
        out.add(norm(p[0]))
        if len(p) > 1 and p[1]:
            out |= {norm(a) for a in p[1].split(";") if a.strip()}
    return out


def main():
    path = sys.argv[1] if len(sys.argv) > 1 else os.path.join(ROOT, "assets", "icons", "transformers_names.txt")
    icons = os.path.join(ROOT, "assets", "icons")
    others = set()
    for f in glob.glob(os.path.join(icons, "*.ron")) + glob.glob(os.path.join(ROOT, "assets", "icons.ron")):
        if os.path.basename(f) != "transformers.ron":
            others |= ron_names(f)
    for f in glob.glob(os.path.join(icons, "*_names.txt")):
        if os.path.abspath(f) != os.path.abspath(path):
            others |= txt_names(f)
    mine = ron_names(os.path.join(icons, "transformers.ron"))
    seen = {}
    errs = 0
    single = []
    n = 0
    for i, ln in enumerate(open(path, encoding="utf-8"), 1):
        if not ln.strip() or ln.startswith("#"):
            continue
        n += 1
        p = [x.strip() for x in ln.rstrip("\n").split("|")]
        if len(p) != 4 or not p[3].lower().startswith("hint:"):
            print("%d: bad format" % i)
            errs += 1
            continue
        name, aliases, qid, hint = p
        if "(" in hint or ")" in hint:
            print("%d: parentheses in hint" % i)
            errs += 1
        if qid and not re.fullmatch(r"Q\d+", qid):
            print("%d: bad QID %r" % (i, qid))
            errs += 1
        words = re.sub(r"\[[^\]]*\]", " ", hint)[5:]
        toks = [w for w in re.split(r"[^A-Za-z0-9_-]+", words) if w]
        bad = [w for w in toks if w not in ENUMS]
        if bad:
            print("%d: non-enum hint words %s" % (i, bad))
            errs += 1
        if not any(w in tflib.FORMS for w in toks):
            print("%d: no Form in hint" % i)
            errs += 1
        keys = [norm(name)] + [norm(a) for a in aliases.split(";") if a.strip()]
        for k in keys:
            if k != k.lower() or not k:
                print("%d: bad key %r" % (i, k))
                errs += 1
            if k in seen:
                print("%d: duplicate %r (line %d)" % (i, k, seen[k]))
                errs += 1
            seen[k] = i
            if k in mine:
                print("%d: %r already a hand-authored transformers.ron name" % (i, k))
                errs += 1
            if k in others:
                print("%d: %r clashes with another icon pack" % (i, k))
                errs += 1
            if " " not in k and "-" not in k:
                single.append(k)
    print("%d entries, %d errors" % (n, errs))
    print("single-word keys (review for common words):", " ".join(sorted(set(single))))
    return 1 if errs else 0


if __name__ == "__main__":
    sys.exit(main())
