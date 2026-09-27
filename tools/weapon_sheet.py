"""WEAPON FEEL: stitch BD_WEAPON_TEST frames into the class comparison sheet (docs/weapons/light_vs_heavy.png).

usage: python tools/weapon_sheet.py <fp dir> <tp dir> <out.png> <slug:label> [<slug:label> ...]
Frames are `<slug>_<NN>_<tag>.png` written by `present::weapon_feel::dev` (BD_WEAPON_SHOTS), one run in
first person (BD_WEAPON_CAM=0) and one in third person (`--third-person`). Each row: the label, the three
first-person frames (wind-up / strike / follow-through, or fire / kick / sustain for guns), then the three
third-person frames.
"""
import glob
import os
import sys

from PIL import Image, ImageDraw, ImageFont


def frames(d, slug):
    return sorted(glob.glob(os.path.join(d, f"{slug}_*.png")))[:3]


def font(size):
    for name in ("arialbd.ttf", "arial.ttf"):
        try:
            return ImageFont.truetype(name, size)
        except OSError:
            pass
    return ImageFont.load_default()


def main():
    fp_dir, tp_dir, out = sys.argv[1], sys.argv[2], sys.argv[3]
    rows = [a.split(":", 1) for a in sys.argv[4:]]
    w, h, gap, label_w, head_h = 320, 180, 3, 190, 34
    cols = 6
    sheet = Image.new("RGB", (label_w + cols * (w + gap), head_h + len(rows) * (h + gap)), (16, 16, 18))
    d = ImageDraw.Draw(sheet)
    f_big, f_small = font(20), font(14)
    heads = ["1P wind-up / fire", "1P strike / kick", "1P follow / sustain", "3P wind-up / fire", "3P strike / kick", "3P follow / sustain"]
    for c, t in enumerate(heads):
        d.text((label_w + c * (w + gap) + 8, 8), t, fill=(200, 200, 210), font=f_small)
    for r, (slug, label) in enumerate(rows):
        y = head_h + r * (h + gap)
        name, _, cls = label.partition("|")
        d.text((10, y + h // 2 - 22), name, fill=(255, 255, 255), font=f_big)
        d.text((10, y + h // 2 + 4), cls, fill=(255, 200, 90), font=f_small)
        for c, src in enumerate(frames(fp_dir, slug) + [None] * 0):
            im = Image.open(src).convert("RGB").resize((w, h), Image.LANCZOS)
            sheet.paste(im, (label_w + c * (w + gap), y))
        for c, src in enumerate(frames(tp_dir, slug)):
            im = Image.open(src).convert("RGB").resize((w, h), Image.LANCZOS)
            sheet.paste(im, (label_w + (3 + c) * (w + gap), y))
    os.makedirs(os.path.dirname(out) or ".", exist_ok=True)
    sheet.save(out)
    print(f"{out}: {len(rows)} rows")


if __name__ == "__main__":
    main()
