"""Stitch BD_LEGEND_TEST frames into a labelled horizontal strip (docs/legendary/<scene>_strip.png).

usage: python tools/legend_strip.py <shots dir> <scene> <out.png> [max_frames] [frame_width]
Frames are `<scene>_<NN>_<tag>.png` written by `present::legend::dev` (BD_LEGEND_SHOTS).
"""
import glob
import os
import re
import sys

from PIL import Image, ImageDraw, ImageFont


def main():
    src, scene, out = sys.argv[1], sys.argv[2], sys.argv[3]
    max_frames = int(sys.argv[4]) if len(sys.argv) > 4 else 6
    width = int(sys.argv[5]) if len(sys.argv) > 5 else 640
    files = sorted(glob.glob(os.path.join(src, f"{scene}_*.png")))
    if not files:
        sys.exit(f"no frames for {scene} in {src}")
    if len(files) > max_frames:
        # Keep the first and last, spread the rest evenly.
        idx = [round(i * (len(files) - 1) / (max_frames - 1)) for i in range(max_frames)]
        files = [files[i] for i in sorted(set(idx))]
    frames = []
    for f in files:
        im = Image.open(f).convert("RGB")
        h = round(im.height * width / im.width)
        im = im.resize((width, h), Image.LANCZOS)
        tag = re.sub(rf"^{re.escape(scene)}_\d+_", "", os.path.splitext(os.path.basename(f))[0])
        d = ImageDraw.Draw(im)
        try:
            font = ImageFont.truetype("arial.ttf", 18)
        except OSError:
            font = ImageFont.load_default()
        d.rectangle([0, 0, 12 + 10 * len(tag), 26], fill=(0, 0, 0))
        d.text((6, 3), tag, fill=(255, 255, 255), font=font)
        frames.append(im)
    gap = 4
    strip = Image.new("RGB", (sum(f.width for f in frames) + gap * (len(frames) - 1), max(f.height for f in frames)), (20, 20, 20))
    x = 0
    for f in frames:
        strip.paste(f, (x, 0))
        x += f.width + gap
    os.makedirs(os.path.dirname(out) or ".", exist_ok=True)
    strip.save(out)
    print(f"{out}: {len(frames)} frames")


if __name__ == "__main__":
    main()
