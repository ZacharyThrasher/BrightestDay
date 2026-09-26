"""Render the two-mode contact sheet with the real bd_shapes voxeliser (tools/icons/transformers/tfview).

    python tools/icons/transformers/render.py [id,id,...] [--out PATH] [--tile N]
"""
import os
import subprocess
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.abspath(os.path.join(HERE, "..", "..", ".."))

args = sys.argv[1:]
only = None
out = os.path.join(os.environ.get("TF_SCRATCH", HERE), "tf_sheet.png")
tile = "200"
i = 0
while i < len(args):
    if args[i] == "--out":
        out = args[i + 1]
        i += 2
    elif args[i] == "--tile":
        tile = args[i + 1]
        i += 2
    else:
        only = args[i]
        i += 1
env = dict(os.environ)
env["ICON_FILE"] = os.path.join(ROOT, "assets", "icons", "transformers.ron")
env.setdefault("CARGO_TARGET_DIR", os.path.join(ROOT, "target", "icons"))
if only:
    env["ICON_ONLY"] = only
subprocess.run(["cargo", "run", "-q", "--release", "--manifest-path", os.path.join(HERE, "tfview", "Cargo.toml"), "--",
                "--out", out, "--tile", tile], env=env, check=True)
