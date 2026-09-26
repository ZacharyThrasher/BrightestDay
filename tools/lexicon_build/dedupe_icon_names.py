"""Remove icon names that an earlier-loading icon file already owns (the loader keeps the earlier one
and warns). Load order = bd_lexicon's: assets/icons.ron, then assets/icons/*.ron sorted by file name.
Names are compared by normalised key (same rules as bd_lexicon::normalize_key), so "bob-omb" and
"bob omb" collide. An entry left with no names is removed.

    python tools/lexicon_build/dedupe_icon_names.py            # dry run: list what would change
    python tools/lexicon_build/dedupe_icon_names.py --write    # rewrite the pack files

Icon generators under tools/icons/* should filter with `taken_before(<pack file name>)` so a
regeneration stays warning-free.
"""
from __future__ import annotations

import re
import sys
import unicodedata
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent.parent
ASSETS = ROOT / "assets"
_POSS = re.compile(r"(?<=[a-z0-9])['’]s(?![a-z0-9])")
_NONALNUM = re.compile(r"[^a-z0-9]+")
_NAMES = re.compile(r"(\bnames:\s*\[)(.*?)(\])", re.S)


# Names a specific pack owns even though an earlier-loading pack also lists them (lead decision, see
# tools/icons/internet/gen.py KEEP): they are removed from every OTHER pack instead.
OWNED_BY = {k: "internet.ron" for k in [
    "amogus", "among us", "amongus", "among us crewmate", "crewmate", "impostor", "imposter", "sus", "amogus sus",
    "banana peel"]}
# Hand-shaped Pokemon items live in pokemon.ron (Pokemon engineer); games.ron's procedural copies yield.
OWNED_BY.update({k: "pokemon.ron" for k in ["master ball", "masterball", "rare candy"]})


def norm(s: str) -> str:
    """Mirror of bd_lexicon::normalize_key."""
    s = s.lower().replace("♀", " f ").replace("♂", " m ")
    s = unicodedata.normalize("NFKD", s)
    s = "".join(c for c in s if not unicodedata.combining(c))
    s = s.replace("ß", "ss").replace("æ", "ae").replace("œ", "oe").replace("ø", "o")
    s = _POSS.sub("", s)
    s = s.replace("'", "").replace("’", "").replace("`", "")
    return _NONALNUM.sub(" ", s).strip()


def load_order() -> list[Path]:
    files = [ASSETS / "icons.ron"] if (ASSETS / "icons.ron").exists() else []
    return files + sorted((ASSETS / "icons").glob("*.ron"))


def names_in(text: str) -> list[str]:
    out = []
    for m in _NAMES.finditer(text):
        out += re.findall(r'"((?:[^"\\]|\\.)*)"', m.group(2))
    return out


def taken_before(pack_name: str) -> set[str]:
    """Normalised names owned by files that load before `pack_name` (e.g. "games.ron")."""
    taken: set[str] = set()
    for f in load_order():
        if f.name == pack_name and f.parent.name == "icons":
            break
        taken.update(norm(n) for n in names_in(f.read_text(encoding="utf-8")))
    return taken


def split_top_entries(text: str) -> list[tuple[int, int]]:
    """(start, end) spans of the top-level list's entries, honouring strings/comments/nesting."""
    spans, depth, i, n, start, in_list = [], 0, 0, len(text), None, False
    while i < n:
        c = text[i]
        if c == '"':
            i += 1
            while i < n and text[i] != '"':
                i += 2 if text[i] == "\\" else 1
        elif text.startswith("//", i):
            while i < n and text[i] != "\n":
                i += 1
            continue
        elif c in "([{":
            if not in_list:
                if c == "[":
                    in_list = True
            else:
                if depth == 0:
                    start = i if start is None else start
                depth += 1
        elif c in ")]}":
            if in_list and depth == 0 and c == "]":
                break
            depth -= 1
            if in_list and depth == 0 and start is not None:
                spans.append((start, i + 1))
                start = None
        elif in_list and depth == 0 and start is None and (c.isalpha()):
            start = i  # struct name "IconEntry("
        i += 1
    return spans


CR, LF = chr(13), chr(10)
CRLF = CR + LF


def dedupe(path: Path, write: bool) -> tuple[int, int]:
    raw = path.read_bytes().decode("utf-8")
    crlf = CRLF in raw
    text = raw.replace(CRLF, LF)
    taken = taken_before(path.name) if path.parent.name == "icons" else set()
    seen = {k: None for k in taken}  # key -> entry index that owns it (None = an earlier file)
    removed_names = dropped = 0
    pieces, last = [], 0
    for idx, (s, e) in enumerate(split_top_entries(text)):
        entry = text[s:e]
        m = _NAMES.search(entry)
        if not m:
            continue
        names = re.findall(r'"((?:[^"\\]|\\.)*)"', m.group(2))
        keep = []
        for nm in names:
            k = norm(nm)
            if not k:  # e.g. an emoji: the loader ignores it, nothing to collide with
                keep.append(nm)
                continue
            pinned = OWNED_BY.get(k)
            if pinned is not None and pinned != path.name:
                continue  # another pack owns this name
            owner = idx if pinned == path.name else seen.get(k, idx)
            if owner == idx:  # free, or a spelling variant within this same entry
                keep.append(nm)
                seen[k] = idx
        if len(keep) == len(names):
            continue
        removed_names += len(names) - len(keep)
        print(f"  {path.name}: [{names[0]}] -{[x for x in names if x not in keep]}" + ("  (entry dropped)" if not keep else ""))
        # cut [last, s) as is, then the edited/dropped entry
        pieces.append(text[last:s])
        if keep:
            new_list = ", ".join('"' + x + '"' for x in keep)
            pieces.append(entry[:m.start(2)] + new_list + entry[m.end(2):])
        else:
            dropped += 1
            # drop the entry plus its trailing comma / line
            tail = text[e:e + 2]
            e += 1 if tail.startswith(",") else 0
            while pieces and pieces[-1].endswith((" ", "\t")):
                pieces[-1] = pieces[-1].rstrip(" \t")
        last = e
    pieces.append(text[last:])
    if write and (removed_names or dropped):
        out = "".join(pieces)
        path.write_bytes((out.replace(LF, CRLF) if crlf else out).encode("utf-8"))  # keep line endings
    return removed_names, dropped


def main():
    sys.stdout.reconfigure(encoding="utf-8")
    write = "--write" in sys.argv
    only = [a for a in sys.argv[1:] if not a.startswith("--")]
    for f in load_order():
        if only and f.name not in only:
            continue
        r, d = dedupe(f, write)
        if r or d:
            print(f"{f.relative_to(ROOT)}: {r} duplicate names removed, {d} entries dropped" + ("" if write else " (dry run)"))


if __name__ == "__main__":
    main()
