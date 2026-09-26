"""Sakura Keep layout tracer (private use: the floor plan follows Overwatch's Hanamura — see
THIRD_PARTY_NOTICES.md "Maps"; replace before any public release).

    python tools/maps/sakura_trace.py <reference top-down layout image> [--overlay out.png]

1. Extracts the playable outline from the owner-supplied top-down layout image (dark neutral
   background flood-filled from the border; everything else is the map) as per-row pixel runs.
2. Holds the hand-authored layout spec (terraces, stairs, buildings, trees, lanterns...) in the
   image's pixel frame (x right, y down).
3. Writes `crates/bd_voxel/src/maps/sakura_layout.rs` (GENERATED) and an overlay of the spec on
   the reference (default `target/sakura/trace_overlay.png`) to check the trace.

The reference image itself is not in the repository. The castle interior is authored in Rust
(`sakura.rs`) in the castle's rotated frame (CASTLE_* below, mirrored there).
"""

import math
import sys
from pathlib import Path

import numpy as np
from PIL import Image, ImageDraw
from scipy import ndimage as ndi

ROOT = Path(__file__).resolve().parents[2]

# ---- frame (mirrored in sakura.rs) ---------------------------------------------------------------
# World metres: x = (px - OX) * S, z = (OY - py) * S.
S = 0.26
OX, OY = 368.0, 380.0
# Castle frame: origin (the keep hall's centre), angle of its +u axis (degrees, image frame).
CASTLE_O = (615.0, 165.0)
CASTLE_DEG = 27.0

# Heights (m).
H_STREET = 10.0
H_PLAZA = 11.5
H_A = 13.0
H_AROOM = 14.0
H_GARDEN = 14.5
H_LOW = 13.0
H_CASTLE = 17.0


def orect(cx, cy, w, h, deg):
    """Rotated rectangle (image px) as 4 points."""
    a = math.radians(deg)
    c, s = math.cos(a), math.sin(a)
    pts = []
    for (u, v) in [(-w / 2, -h / 2), (w / 2, -h / 2), (w / 2, h / 2), (-w / 2, h / 2)]:
        pts.append((round(cx + u * c - v * s, 1), round(cy + u * s + v * c, 1)))
    return pts


def castle(u, v):
    """Castle-local (u, v) px -> image px."""
    a = math.radians(CASTLE_DEG)
    c, s = math.cos(a), math.sin(a)
    return (round(CASTLE_O[0] + u * c - v * s, 1), round(CASTLE_O[1] + u * s + v * c, 1))


def crect(u0, v0, u1, v1):
    return [castle(u0, v0), castle(u1, v0), castle(u1, v1), castle(u0, v1)]


# Castle facade (SW wall of the keep's base): the line through these points; the castle is on its
# north side (x >= 398).
FACADE = ((490.0, 190.0), (685.0, 300.0))


def facade_at(x):
    (ax, ay), (bx, by) = FACADE
    y = ay + (x - ax) * (by - ay) / (bx - ax)
    L = math.hypot(bx - ax, by - ay)
    d = ((bx - ax) / L, (by - ay) / L)
    n = (-d[1], d[0])  # towards the garden (image y down)
    return (x, y), d, n


def keep_steps(x, half, depth=18.0):
    """Stone steps up the keep's base at facade x: `2*half` px wide, `depth` px deep into the garden.
    Returns (poly, low end, high end)."""
    f, d, n = facade_at(x)
    pts = [(f[0] - d[0] * half - n[0], f[1] - d[1] * half - n[1]), (f[0] + d[0] * half - n[0], f[1] + d[1] * half - n[1]),
           (f[0] + d[0] * half + n[0] * depth, f[1] + d[1] * half + n[1] * depth),
           (f[0] - d[0] * half + n[0] * depth, f[1] - d[1] * half + n[1] * depth)]
    lo = (round(f[0] + n[0] * (depth - 1), 1), round(f[1] + n[1] * (depth - 1), 1))
    hi = (round(f[0] - n[0], 1), round(f[1] - n[1], 1))
    return [(round(a, 1), round(b, 1)) for a, b in pts], lo, hi


def facade_door(x, half):
    f, d, _ = facade_at(x)
    return ((round(f[0] - d[0] * half, 1), round(f[1] - d[1] * half, 1)), (round(f[0] + d[0] * half, 1), round(f[1] + d[1] * half, 1)))


# ---- spec ------------------------------------------------------------------------------------------
# Terraces: flat ground height inside the polygon (later entries win). Default: H_STREET.
TERRACES = [
    # First-gate plaza (the raised square north of the spawn street) and the SE lane's foot.
    ("plaza", [(140, 445), (376, 445), (376, 520), (456, 520), (456, 582), (380, 582), (346, 568), (250, 580), (196, 600), (140, 600)], H_PLAZA),
    # A district: north street, west street, A courtyard, the long south arcade, middle street.
    ("a_district", [(15, 150), (398, 150), (398, 395), (340, 440), (240, 440), (160, 452), (15, 470)], H_A),
    # The SE lane between the ravine and the lower garden.
    ("se_lane", [(376, 440), (482, 440), (482, 472), (376, 472)], H_LOW),
    # Garden (the sakura court) up to the castle's stone base.
    ("garden", [(365, 225), (500, 190), (600, 245), (700, 300), (640, 430), (560, 430), (470, 440), (365, 440)], H_GARDEN),
    # Lower garden (south-east, rock garden with the statue) and the east plaza.
    ("low_garden", [(440, 432), (650, 425), (650, 530), (440, 530)], H_LOW),
    ("east_plaza", [(595, 325), (705, 325), (705, 440), (620, 440), (595, 420)], H_GARDEN),
]

_KW = keep_steps(530.0, 20.0)
_KE = keep_steps(640.0, 18.0)
# Stairs / ramps: poly, from point (h0) to point (h1); height interpolates along a->b, clamped,
# quantised to 0.5 m risers by the voxels. `mat`: stone / wood.
STAIRS = [
    # Spawn street -> plaza: west steps, centre steps, east steps.
    ("west_steps", [(150, 520), (192, 520), (192, 606), (150, 606)], (170, 604), H_STREET, (170, 560), H_PLAZA, "stone"),
    ("centre_steps", [(196, 572), (262, 560), (262, 596), (196, 606)], (230, 602), H_STREET, (230, 574), H_PLAZA, "stone"),
    ("east_steps", [(380, 578), (432, 574), (434, 600), (382, 606)], (406, 603), H_STREET, (406, 578), H_PLAZA, "stone"),
    # Plaza -> A district: the first gate (stone steps) and the west lane.
    ("gate_steps", [(236, 428), (282, 424), (284, 470), (238, 474)], (260, 472), H_PLAZA, (260, 430), H_A, "stone"),
    ("west_lane", [(96, 432), (170, 425), (170, 470), (96, 478)], (130, 474), H_PLAZA, (130, 432), H_A, "stone"),
    # A courtyard -> A room (the wooden steps up into A from the south).
    ("a_steps", [(158, 362), (200, 356), (206, 410), (164, 416)], (182, 412), H_A, (180, 362), H_AROOM, "wood"),
    # SE lane: plaza level -> lane level (north), between the blocks and the lower garden.
    ("se_ramp", [(410, 470), (482, 470), (482, 522), (410, 522)], (446, 520), H_PLAZA, (446, 472), H_LOW, "stone"),
    # Middle street -> garden: the west bridge (wood) and the south steps (from the SE lane).
    ("bridge", [(352, 318), (398, 318), (398, 358), (352, 358)], (354, 338), H_A, (396, 338), H_GARDEN, "wood"),
    ("garden_south_steps", [(372, 396), (420, 396), (424, 440), (376, 444)], (398, 442), H_LOW, (398, 398), H_GARDEN, "stone"),
    # Lower garden -> east plaza.
    ("plaza_e_steps", [(600, 416), (650, 416), (650, 444), (600, 444)], (625, 442), H_LOW, (625, 420), H_GARDEN, "stone"),
    # Garden -> the keep's stone base; middle street -> the castle's NW wing.
    ("keep_steps_w", _KW[0], _KW[1], H_GARDEN, _KW[2], H_CASTLE, "stone"),
    ("keep_steps_e", _KE[0], _KE[1], H_GARDEN, _KE[2], H_CASTLE, "stone"),
    ("keep_steps_nw", [(376, 150), (399, 150), (399, 184), (376, 184)], (377, 167), H_A, (400, 167), H_CASTLE, "stone"),
]
# Doors in the castle facade, at the top of each keep stair.
CASTLE_DOORS = [facade_door(530.0, 11.0), facade_door(640.0, 10.0)]
# Gravel paths through the garden (polylines px, width m) and raked-gravel courts.
PATHS = [
    ([(398, 338), (432, 332)], 3.0), ([(398, 400), (438, 352)], 3.0), ([(488, 316), (528, 232)], 3.5),
    ([(484, 340), (560, 382)], 3.0), ([(628, 432), (604, 310), (638, 282)], 3.0),
]
GRAVEL = [[(200, 335), (345, 330), (345, 408), (205, 415)]]

# Buildings. kind: "block" = closed (sealed, roofed), "hall" = enterable (doors), "arcade" = roof on
# posts. style: plaster / timber / temple / stone / shop. roof: hip / gable / flat.
# doors: list of ((ax, ay), (bx, by)) wall segments to open, in px.
BUILDINGS = [
    # ---- south: first-gate plaza blocks + the shop at the spawn street -------------------------
    dict(name="plaza_block_c", kind="block", poly=[(245, 529), (304, 512), (310, 541), (251, 557)], floor=H_PLAZA, wall=6.5, roof="hip", style="plaster"),
    dict(name="plaza_block_ne", kind="block", poly=[(339, 512), (401, 492), (410, 518), (345, 532)], floor=H_PLAZA, wall=7.5, roof="gable", style="timber"),
    dict(name="plaza_block_e", kind="block", poly=[(360, 555), (409, 544), (415, 575), (366, 588)], floor=H_PLAZA, wall=6.0, roof="hip", style="plaster"),
    dict(name="shop", kind="hall", poly=[(265, 585), (331, 565), (346, 617), (282, 634)], floor=H_STREET, wall=6.0, roof="hip", style="shop",
         doors=[((278, 630), (314, 620)), ((268, 592), (272, 606)), ((326, 568), (334, 580))]),
    # ---- A district -------------------------------------------------------------------------------
    # The A hall: tall main room (the old point) + west wooden wing + east walkway, one roof line.
    dict(name="a_hall", kind="hall", poly=orect(183, 301, 100, 100, -15), floor=H_AROOM, wall=12.0, roof="hip", style="temple",
         doors=[((150, 358), (200, 345)), ((129, 285), (136, 330)), ((215, 250), (228, 290)), ((236, 316), (240, 332))],
         gallery=4.5),
    dict(name="a_west_wing", kind="hall", poly=[(74, 290), (130, 275), (152, 360), (96, 370)], floor=H_A, wall=8.0, roof="gable", style="timber",
         doors=[((80, 312), (86, 340)), ((100, 368), (140, 362)), ((126, 285), (134, 318))], gallery=4.0),
    dict(name="a_sw_room", kind="hall", poly=[(80, 372), (148, 360), (156, 405), (90, 418)], floor=H_A, wall=5.0, roof="hip", style="plaster",
         doors=[((84, 380), (86, 400)), ((150, 372), (154, 392))]),
    dict(name="north_block", kind="block", poly=orect(300, 226, 44, 18, -15), floor=H_A, wall=6.0, roof="gable", style="timber"),
    dict(name="middle_house", kind="hall", poly=orect(316, 286, 58, 84, -15), floor=H_A, wall=7.0, roof="hip", style="plaster",
         doors=[((292, 252), (304, 248)), ((302, 330), (316, 326)), ((287, 270), (292, 300))]),
    dict(name="south_arcade", kind="arcade", poly=[(98, 420), (236, 400), (240, 428), (102, 450)], floor=H_A, wall=4.5, roof="gable", style="timber"),
    dict(name="se_room", kind="hall", poly=orect(312, 423, 64, 44, -15), floor=H_A, wall=5.5, roof="hip", style="plaster",
         doors=[((282, 412), (286, 432)), ((332, 400), (346, 396))]),
    # ---- garden ---------------------------------------------------------------------------------
    dict(name="garden_nw_room", kind="hall", poly=[(368, 252), (436, 238), (442, 288), (374, 296)], floor=H_GARDEN, wall=5.5, roof="hip", style="plaster",
         doors=[((372, 262), (374, 286)), ((404, 292), (428, 290))]),
    dict(name="temple", kind="hall", poly=[(490, 382), (566, 336), (606, 372), (604, 416), (572, 422), (492, 400)], floor=H_GARDEN, wall=8.0, roof="hip", style="temple",
         doors=[((494, 384), (520, 368)), ((604, 380), (604, 408)), ((534, 408), (568, 418))]),
    dict(name="garden_wall_s", kind="block", poly=[(478, 428), (548, 420), (550, 440), (480, 450)], floor=H_LOW, wall=4.0, roof="gable", style="plaster"),
]

# Sakura trees (px, py, height m) and other greenery (green pines: "pine").
TREES = [
    (415, 302, 9.0, "sakura"), (427, 367, 10.0, "sakura"), (410, 398, 8.0, "sakura"), (515, 350, 9.5, "sakura"),
    (207, 497, 7.0, "pine"), (535, 465, 7.5, "pine"), (606, 470, 6.5, "pine"), (132, 650, 6.0, "pine"), (160, 648, 6.5, "pine"),
    (365, 648, 6.0, "sakura"), (90, 305, 6.0, "pine"), (300, 460, 5.5, "pine"),
]

# The shrine (octagonal pagoda + koi pond ring) in the garden.
SHRINE = dict(center=(462, 325), radius_px=22.0, pond_px=34.0)

# Stone lanterns (px, py) and hanging paper lantern strings ((ax, ay), (bx, by), height m).
STONE_LANTERNS = [(628, 392), (440, 330), (484, 300), (250, 600), (320, 440), (60, 420), (690, 360)]
PAPER_STRINGS = [((140, 640), (370, 620), 5.0), ((110, 250), (300, 200), 5.5), ((330, 180), (360, 380), 5.0)]

# Torii gates ((ax, ay), (bx, by)) = the two posts.
TORII = [((392, 404), (412, 402)), ((236, 452), (260, 450)), ((580, 408), (596, 408))]

# Spawn street gatehouse doors (the original attack exits; closed gates).
GATES = [((176, 716), (194, 716)), ((306, 700), (324, 700))]

# Open edges: no perimeter building ring here (a low railing and the drop to the void instead).
OPEN_EDGES = [
    [(330, 370), (390, 370), (395, 505), (330, 505)],     # the ravine between the A district and the garden
    [(420, 440), (480, 440), (480, 500), (420, 500)],     # its eastern arm (lower garden overlook)
    [(560, 480), (700, 440), (720, 530), (560, 540)],     # lower garden south cliff
    [(695, 300), (740, 300), (740, 440), (695, 440)],     # east plaza
    [(0, 250), (32, 250), (32, 470), (0, 470)],           # west street
]

# Castle footprint (image px) — the region the Rust castle builder owns (stone base at H_CASTLE).
CASTLE_REGION = [(398, 0), (760, 0), (760, 330), FACADE[1], FACADE[0], (398, 190)]

# FFA spawns (px, py): spread, never in a chokepoint.
SPAWNS = [
    (150, 680), (345, 668), (215, 500), (55, 300), (60, 440), (250, 210), (520, 470), (650, 380), (470, 110), (590, 60),
]


# ---- outline -------------------------------------------------------------------------------------
def outline(img_path):
    im = np.asarray(Image.open(img_path).convert("RGB")).astype(int)
    mx, mn = im.max(2), im.min(2)
    dark = (mx < 62) & ((mx - mn) < 22)
    h, w = dark.shape
    # The legend box (top left) and the credit (bottom right) are not map.
    dark[0:140, 0:280] = True
    dark[680:h, 560:w] = True
    lab, _ = ndi.label(dark)
    ext = set(lab[0, :]) | set(lab[-1, :]) | set(lab[:, 0]) | set(lab[:, -1])
    ext.discard(0)
    inside = ~np.isin(lab, list(ext))
    inside = ndi.binary_closing(inside, iterations=2)
    inside = ndi.binary_opening(inside, iterations=2)
    # The image frame lines are not map.
    inside[:, :14] = False
    inside[:, 736:] = False
    inside[:10, :] = False
    inside[722:, :] = False
    lab2, n = ndi.label(inside)
    sizes = ndi.sum(inside, lab2, range(1, n + 1))
    keep = 1 + int(np.argmax(sizes))
    return lab2 == keep


def runs(mask):
    rows = []
    for y in range(mask.shape[0]):
        r = mask[y]
        if not r.any():
            continue
        d = np.diff(np.concatenate([[0], r.astype(np.int8), [0]]))
        starts = np.nonzero(d == 1)[0]
        ends = np.nonzero(d == -1)[0]
        rows.append((y, list(zip(starts.tolist(), ends.tolist()))))
    return rows


# ---- overlay ---------------------------------------------------------------------------------------
def overlay(img_path, mask, out):
    ref = Image.open(img_path).convert("RGB")
    k = 2
    base = np.asarray(ref).astype(float) * 0.6
    base[~mask] *= 0.35
    img = Image.fromarray(base.clip(0, 255).astype(np.uint8)).resize((ref.width * k, ref.height * k), Image.NEAREST)
    d = ImageDraw.Draw(img, "RGBA")
    sc = lambda pts: [(x * k, y * k) for (x, y) in pts]
    for name, poly, hgt in TERRACES:
        d.polygon(sc(poly), outline=(80, 200, 255, 255), fill=(80, 200, 255, 25))
        cx = sum(p[0] for p in poly) / len(poly)
        cy = sum(p[1] for p in poly) / len(poly)
        d.text((cx * k, cy * k), f"{name} {hgt:g}", fill=(120, 220, 255, 255))
    for name, poly, a, h0, b, h1, _ in STAIRS:
        d.polygon(sc(poly), outline=(255, 230, 60, 255), fill=(255, 230, 60, 50))
        d.line(sc([a, b]), fill=(255, 255, 0, 255), width=3)
        d.text((b[0] * k, b[1] * k), f"{h1:g}", fill=(255, 255, 0, 255))
    col = {"block": (255, 60, 60), "hall": (255, 150, 40), "arcade": (200, 120, 255)}
    for b in BUILDINGS:
        c = col[b["kind"]]
        d.polygon(sc(b["poly"]), outline=c + (255,), fill=c + (60,))
        for (p, q) in b.get("doors", []):
            d.line(sc([p, q]), fill=(60, 255, 60, 255), width=5)
        cx = sum(p[0] for p in b["poly"]) / len(b["poly"])
        cy = sum(p[1] for p in b["poly"]) / len(b["poly"])
        d.text((cx * k - 20, cy * k), b["name"], fill=(255, 255, 255, 255))
    d.polygon(sc(CASTLE_REGION), outline=(255, 255, 255, 255))
    for u0, v0, u1, v1 in [(-50, -45, 50, 45)]:
        d.polygon(sc(crect(u0, v0, u1, v1)), outline=(255, 200, 200, 255))
    for poly in OPEN_EDGES:
        d.polygon(sc(poly), outline=(0, 0, 0, 255), fill=(0, 0, 0, 90))
    for (x, y, hh, kind) in TREES:
        r = 4 + hh
        c = (255, 120, 200, 200) if kind == "sakura" else (60, 200, 90, 200)
        d.ellipse([(x - r) * k, (y - r) * k, (x + r) * k, (y + r) * k], outline=c, width=3)
    cx, cy = SHRINE["center"]
    for r, c in [(SHRINE["radius_px"], (255, 255, 255, 255)), (SHRINE["pond_px"], (60, 160, 255, 255))]:
        d.ellipse([(cx - r) * k, (cy - r) * k, (cx + r) * k, (cy + r) * k], outline=c, width=2)
    for (p, wd) in PATHS:
        d.line(sc(p), fill=(230, 220, 180, 160), width=max(1, int(wd / S * k)))
    for poly in GRAVEL:
        d.polygon(sc(poly), outline=(230, 220, 180, 255))
    for (a, b) in CASTLE_DOORS:
        d.line(sc([a, b]), fill=(60, 255, 60, 255), width=6)
    d.line(sc(list(FACADE)), fill=(255, 255, 255, 255), width=1)
    for (x, y) in STONE_LANTERNS:
        d.rectangle([(x - 2) * k, (y - 2) * k, (x + 2) * k, (y + 2) * k], fill=(255, 200, 80, 255))
    for (a, b, _) in PAPER_STRINGS:
        d.line(sc([a, b]), fill=(255, 90, 60, 255), width=2)
    for (a, b) in TORII:
        d.line(sc([a, b]), fill=(255, 40, 40, 255), width=6)
    for i, (x, y) in enumerate(SPAWNS):
        d.ellipse([(x - 5) * k, (y - 5) * k, (x + 5) * k, (y + 5) * k], fill=(255, 30, 30, 255), outline=(255, 255, 255, 255))
        d.text(((x + 6) * k, (y - 6) * k), str(i), fill=(255, 255, 255, 255))
    out.parent.mkdir(parents=True, exist_ok=True)
    img.save(out)
    print("overlay", out)


# ---- Rust ------------------------------------------------------------------------------------------
def fpt(p):
    return f"({p[0]:.1f}, {p[1]:.1f})"


def fpoly(poly):
    return "&[" + ", ".join(fpt(p) for p in poly) + "]"


def emit(mask, out):
    rows = runs(mask)
    L = []
    w = L.append
    w("//! GENERATED by `python tools/maps/sakura_trace.py <layout image>` — do not edit by hand; edit the")
    w("//! spec in the script and regenerate. Sakura Keep's floor plan (image pixel frame: x right, y down;")
    w("//! world metres via `sakura::px_to_m`). PRIVATE USE: the layout follows Overwatch's Hanamura (see")
    w("//! THIRD_PARTY_NOTICES.md \"Maps\"); replace it before any public release.")
    w("")
    w("#![allow(clippy::approx_constant)]")
    w("")
    w("/// Metres per image pixel and the image point at world (0, 0).")
    w(f"pub const S: f32 = {S};")
    w(f"pub const OX: f32 = {OX:.1};")
    w(f"pub const OY: f32 = {OY:.1};")
    w("/// The castle's frame: origin (the keep hall's centre, px) and its +u axis angle (degrees, image frame).")
    w(f"pub const CASTLE_O: (f32, f32) = ({CASTLE_O[0]:.1}, {CASTLE_O[1]:.1});")
    w(f"pub const CASTLE_DEG: f32 = {CASTLE_DEG:.1};")
    for nm, v in [("H_STREET", H_STREET), ("H_PLAZA", H_PLAZA), ("H_A", H_A), ("H_AROOM", H_AROOM), ("H_GARDEN", H_GARDEN), ("H_LOW", H_LOW), ("H_CASTLE", H_CASTLE)]:
        w(f"pub const {nm}: f32 = {v:.1};")
    w("")
    w("#[derive(Clone, Copy, Debug, PartialEq, Eq)]")
    w("pub enum StairMat { Stone, Wood }")
    w("#[derive(Clone, Copy, Debug, PartialEq, Eq)]")
    w("pub enum Kind { Block, Hall, Arcade }")
    w("#[derive(Clone, Copy, Debug, PartialEq, Eq)]")
    w("pub enum Roof { Hip, Gable, Flat }")
    w("#[derive(Clone, Copy, Debug, PartialEq, Eq)]")
    w("pub enum Style { Plaster, Timber, Temple, Stone, Shop }")
    w("")
    w("pub struct Terrace { pub name: &'static str, pub poly: &'static [(f32, f32)], pub h: f32 }")
    w("pub struct Stair { pub name: &'static str, pub poly: &'static [(f32, f32)], pub a: (f32, f32), pub h0: f32, pub b: (f32, f32), pub h1: f32, pub mat: StairMat }")
    w("pub struct Building {")
    w("    pub name: &'static str,")
    w("    pub kind: Kind,")
    w("    pub poly: &'static [(f32, f32)],")
    w("    pub floor: f32,")
    w("    pub wall: f32,")
    w("    pub roof: Roof,")
    w("    pub style: Style,")
    w("    pub doors: &'static [((f32, f32), (f32, f32))],")
    w("    /// Height of an interior gallery floor above `floor` (halls with a mezzanine), 0 = none.")
    w("    pub gallery: f32,")
    w("}")
    w("pub enum TreeKind { Sakura, Pine }")
    w("")
    w("/// Playable outline: per image row, `[x0, x1)` pixel runs.")
    w("#[rustfmt::skip]")
    w("pub static OUTLINE: &[(u16, &[(u16, u16)])] = &[")
    for y, rs in rows:
        w(f"    ({y}, &[" + ", ".join(f"({a}, {b})" for a, b in rs) + "]),")
    w("];")
    w("")
    w("#[rustfmt::skip]")
    w("pub static TERRACES: &[Terrace] = &[")
    for name, poly, hgt in TERRACES:
        w(f"    Terrace {{ name: {name!r}, poly: {fpoly(poly)}, h: {hgt:.1} }},".replace("'", '"'))
    w("];")
    w("")
    w("#[rustfmt::skip]")
    w("pub static STAIRS: &[Stair] = &[")
    for name, poly, a, h0, b, h1, m in STAIRS:
        mm = "StairMat::Stone" if m == "stone" else "StairMat::Wood"
        w(f"    Stair {{ name: \"{name}\", poly: {fpoly(poly)}, a: {fpt(a)}, h0: {h0:.1}, b: {fpt(b)}, h1: {h1:.1}, mat: {mm} }},")
    w("];")
    w("")
    w("#[rustfmt::skip]")
    w("pub static BUILDINGS: &[Building] = &[")
    for b in BUILDINGS:
        kind = {"block": "Kind::Block", "hall": "Kind::Hall", "arcade": "Kind::Arcade"}[b["kind"]]
        roof = {"hip": "Roof::Hip", "gable": "Roof::Gable", "flat": "Roof::Flat"}[b["roof"]]
        style = {"plaster": "Style::Plaster", "timber": "Style::Timber", "temple": "Style::Temple", "stone": "Style::Stone", "shop": "Style::Shop"}[b["style"]]
        doors = "&[" + ", ".join(f"({fpt(p)}, {fpt(q)})" for p, q in b.get("doors", [])) + "]"
        w(f"    Building {{ name: \"{b['name']}\", kind: {kind}, poly: {fpoly(b['poly'])}, floor: {b['floor']:.1}, wall: {b['wall']:.1}, roof: {roof}, style: {style}, doors: {doors}, gallery: {b.get('gallery', 0.0):.1} }},")
    w("];")
    w("")
    w("/// (px, py, height m, kind).")
    w("#[rustfmt::skip]")
    w("pub static TREES: &[(f32, f32, f32, TreeKind)] = &[")
    for (x, y, hh, kind) in TREES:
        w(f"    ({x:.1}, {y:.1}, {hh:.1}, TreeKind::{'Sakura' if kind == 'sakura' else 'Pine'}),")
    w("];")
    w("")
    w(f"/// The garden shrine: centre (px), pagoda radius (px), koi pond radius (px).")
    w(f"pub const SHRINE: ((f32, f32), f32, f32) = ({fpt(SHRINE['center'])}, {SHRINE['radius_px']:.1}, {SHRINE['pond_px']:.1});")
    w("pub static STONE_LANTERNS: &[(f32, f32)] = &[" + ", ".join(fpt(p) for p in STONE_LANTERNS) + "];")
    w("/// Paper-lantern strings: (from, to, height above the ground m).")
    w("pub static PAPER_STRINGS: &[((f32, f32), (f32, f32), f32)] = &[" + ", ".join(f"({fpt(a)}, {fpt(b)}, {h:.1})" for a, b, h in PAPER_STRINGS) + "];")
    w("/// Torii gates: the two posts.")
    w("pub static TORII: &[((f32, f32), (f32, f32))] = &[" + ", ".join(f"({fpt(a)}, {fpt(b)})" for a, b in TORII) + "];")
    w("/// The spawn street's closed gatehouse doors (in the perimeter ring).")
    w("pub static GATES: &[((f32, f32), (f32, f32))] = &[" + ", ".join(f"({fpt(a)}, {fpt(b)})" for a, b in GATES) + "];")
    w("/// No perimeter building ring inside these (railing + the drop to the void).")
    w("pub static OPEN_EDGES: &[&[(f32, f32)]] = &[" + ", ".join(fpoly(p) for p in OPEN_EDGES) + "];")
    w("/// The castle builder's region (px).")
    w(f"pub static CASTLE_REGION: &[(f32, f32)] = {fpoly(CASTLE_REGION)};")
    w("/// Garden / courtyard gravel paths: polyline (px) and width (m).")
    w("pub static PATHS: &[(&[(f32, f32)], f32)] = &[" + ", ".join(f"({fpoly(p)}, {wd:.1})" for p, wd in PATHS) + "];")
    w("/// Raked-gravel courts (px).")
    w("pub static GRAVEL: &[&[(f32, f32)]] = &[" + ", ".join(fpoly(p) for p in GRAVEL) + "];")
    w("/// The keep's facade line (px): the castle base is on its north side.")
    w(f"pub const FACADE: ((f32, f32), (f32, f32)) = ({fpt(FACADE[0])}, {fpt(FACADE[1])});")
    w("/// Doors in the castle facade (segments, px).")
    w("pub static CASTLE_DOORS: &[((f32, f32), (f32, f32))] = &[" + ", ".join(f"({fpt(a)}, {fpt(b)})" for a, b in CASTLE_DOORS) + "];")
    w("/// FFA spawn targets (px): spread out, never in a chokepoint.")
    w("pub static SPAWNS: &[(f32, f32)] = &[" + ", ".join(fpt(p) for p in SPAWNS) + "];")
    out.write_text("\n".join(L) + "\n", encoding="utf-8")
    print("wrote", out, f"({len(rows)} outline rows)")


def main():
    if len(sys.argv) < 2:
        print(__doc__)
        sys.exit(2)
    img = sys.argv[1]
    ov = ROOT / "target/sakura/trace_overlay.png"
    if "--overlay" in sys.argv:
        ov = Path(sys.argv[sys.argv.index("--overlay") + 1])
    mask = outline(img)
    overlay(img, mask, ov)
    if "--no-emit" not in sys.argv:
        emit(mask, ROOT / "crates/bd_voxel/src/maps/sakura_layout.rs")


if __name__ == "__main__":
    main()
