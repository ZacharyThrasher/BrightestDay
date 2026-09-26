"""Contact sheet of marvel.ron shapes with the real bd_shapes voxeliser (tools/icons/marvel/mview).

    python tools/icons/marvel/render.py [id,id,...] [--out PATH] [--tile N]
"""
import os
import subprocess
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.abspath(os.path.join(HERE, "..", "..", ".."))
args = sys.argv[1:]
only, out, tile = None, os.path.join(os.environ.get("MV_SCRATCH", HERE), "marvel_sheet.png"), "200"
i = 0
while i < len(args):
    if args[i] == "--out":
        out = args[i + 1]; i += 2
    elif args[i] == "--tile":
        tile = args[i + 1]; i += 2
    else:
        only = args[i]; i += 1
env = dict(os.environ)
env["ICON_FILE"] = os.path.join(ROOT, "assets", "icons", "marvel.ron")
env.setdefault("CARGO_TARGET_DIR", os.path.join(ROOT, "target", "icons"))
if only:
    env["ICON_ONLY"] = only
subprocess.run(["cargo", "run", "-q", "--release", "--manifest-path", os.path.join(HERE, "mview", "Cargo.toml"), "--",
                "--icons", "--out", out, "--tile", tile], env=env, check=True)
