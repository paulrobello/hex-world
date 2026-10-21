"""rail_bend_120_center owner review sheet (SKILL.md "Owner checkpoints"): the approved bend
reference, the six-view renders (untextured, evidence only), and in engine with tilt-shift off: the
six-bend circle round the start meadow with its train at play distance, the bend close, the whole
train on the curve, eye level beside the track, 6 m close-ups on the sun and shade sides, and the kit scenario's wide bend meeting a rail
straight. Labelled, 3 columns.
Usage: python3 docs/checkpoints/rail_bend_120/sheet.py (from the repo root; needs Pillow)."""
import pathlib

from PIL import Image, ImageDraw

ROOT = pathlib.Path(__file__).resolve().parents[3]
DOCS = ROOT / "docs/checkpoints/rail_bend_120"
RENDERS = ROOT / "assets-source/kit/rail_bend_120_center/renders"
REFS = ROOT / "assets-source/refs/rail"
W, H = 640, 400
CELLS = [
    (REFS / "ref-2.jpg", "reference ref-2 (approved A17: the rail bend look)"),
    (RENDERS / "top.jpg", "six views: top (edge 0 right, edge 2 lower left)"),
    (RENDERS / "three_quarter.jpg", "six views: three-quarter"),
    (RENDERS / "front.jpg", "six views: front"),
    (RENDERS / "back.jpg", "six views: back"),
    (RENDERS / "left.jpg", "six views: left"),
    (RENDERS / "right.jpg", "six views: right"),
    (DOCS / "distance.jpg", "in play: six wide bends close a 24 m circle, train on it (~75 m)"),
    (DOCS / "close.jpg", "in play: the bend close, sun side (~11 m)"),
    (DOCS / "train.jpg", "in play: engine, carriage, brake third on the curve (~22 m)"),
    (DOCS / "eye.jpg", "in play: eye level 1.7 m, 6.9 m outside the circle"),
    (DOCS / "sun.jpg", "in play: close-up ~6 m, sun side (outside the circle)"),
    (DOCS / "shade.jpg", "in play: close-up ~6 m, shade side (inner ballast slope)"),
    (DOCS / "kit-seam.jpg", "kit scenario: wide bend meets rail straight (world edge 5)"),
    (DOCS / "kit-overview.jpg", "kit scenario: the bend on (-4, 7) off the bridge line"),
]


def cell(path: pathlib.Path, label: str) -> Image.Image:
    im = Image.open(path).convert("RGB")
    im.thumbnail((W, H))
    c = Image.new("RGB", (W, H + 26), (24, 24, 24))
    c.paste(im, ((W - im.width) // 2, 26 + (H - im.height) // 2))
    ImageDraw.Draw(c).text((8, 6), label, fill=(235, 235, 235))
    return c


cells = [cell(p, label) for p, label in CELLS]
cols = 3
rows = (len(cells) + cols - 1) // cols
cw, ch = cells[0].size
sheet = Image.new("RGB", (cw * cols, ch * rows + 30), (24, 24, 24))
ImageDraw.Draw(sheet).text(
    (8, 8),
    "rail_bend_120_center (P3): edge 0 to edge 2 on a 24 m arc, 60-degree turn, 25.133 m; "
    "rail_bend_60's section, 1.6 m gauge, 36 sleepers; 1272/528/152 tris, GLB 86.8 KB, masks 512 KTX2 14-15 KB",
    fill=(235, 235, 235))
for k, c in enumerate(cells):
    sheet.paste(c, ((k % cols) * cw, 30 + (k // cols) * ch))
sheet.save(DOCS / "sheet.jpg", quality=85)
print("wrote", DOCS / "sheet.jpg")
