"""Contact sheet of assets/icons/pokemon.ron with the real bd_shapes voxeliser (reuses tools/icons/dc/dcview).

    python tools/icons/pokemon/render.py [id,id,...] [--rows] [--tile N] [--cols N] [--out PATH]
--rows = FRONT | SIDE | BACK | WALK | SWIPE per icon (use it to check the rig). Ids = first name of the entry.
Default output: tools/icons/pokemon/out/sheet.png
"""
import os
import subprocess
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.abspath(os.path.join(HERE, "..", "..", ".."))
args = sys.argv[1:]
env = dict(os.environ)
env["ICON_FILE"] = os.path.join(ROOT, "assets", "icons", "pokemon.ron")
env.setdefault("CARGO_TARGET_DIR", os.path.join(ROOT, "target", "icons"))
rest = []
out = os.path.join(HERE, "out", "sheet.png")
i = 0
while i < len(args):
    a = args[i]
    if a == "--out":
        out = args[i + 1]
        i += 2
    elif a in ("--tile", "--cols"):
        rest += [a, args[i + 1]]
        i += 2
    elif a.startswith("--"):
        rest.append(a)
        i += 1
    else:
        env["ICON_ONLY"] = a
        i += 1
os.makedirs(os.path.dirname(out), exist_ok=True)
subprocess.run(["cargo", "run", "-q", "--release", "--manifest-path", os.path.join(ROOT, "tools", "icons", "pokemon", "pview", "Cargo.toml"),
                "--", "--out", out] + rest, env=env, check=True)
print("wrote", out)
