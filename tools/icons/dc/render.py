"""Render a contact sheet of assets/icons/dc.ron with the real bd_shapes voxeliser (dcview).

    python tools/icons/dc/render.py [id,id,...] [--rows] [--out PATH] [--tile N] [--cols N]
Two-mode (Transforms) entries get a second tile for their alt_shape.
"""
import os
import subprocess
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.abspath(os.path.join(HERE, "..", "..", ".."))
args = sys.argv[1:]
env = dict(os.environ)
env["ICON_FILE"] = os.path.join(ROOT, "assets", "icons", "dc.ron")
env.setdefault("CARGO_TARGET_DIR", os.path.join(ROOT, "target", "icons"))
rest = []
out = os.path.join(os.environ.get("DC_SCRATCH", HERE), "dc_sheet.png")
i = 0
while i < len(args):
    a = args[i]
    if a in ("--out",):
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
subprocess.run(["cargo", "run", "-q", "--release", "--manifest-path", os.path.join(HERE, "dcview", "Cargo.toml"), "--",
                "--out", out] + rest, env=env, check=True)
