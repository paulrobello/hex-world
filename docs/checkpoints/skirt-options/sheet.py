"""Builds docs/checkpoints/skirt-options/sheet.jpg from target/skirt-opt/shots (see run.py).
Columns: current look, then options 1-4. Rows: the default play camera (cropped to the board), the close
three-quarter view, a crop of its lit wall and a forest_base wall."""
import pathlib

from PIL import Image, ImageDraw, ImageFont

ROOT = pathlib.Path(__file__).resolve().parents[3]
SHOTS = ROOT / "target/skirt-opt/shots"
OUT = ROOT / "docs/checkpoints/skirt-options"
COLS = [
    ("current", "Current (shipped)", "dark topsoil, red clay, gravel"),
    ("1", "O1 Earth", "one earthy soil, no bands, no lip"),
    ("2", "O2 Plinth", "dark matte base edge, grass lip"),
    ("3", "O3 Soft strata", "icon browns, two thin muted seams, lip"),
    ("4", "O4 Cutaway", "lip, root-dark top, darkens to the foot"),
]
# Crop boxes on the 1600x1000 shots: the play view's board, and the close view's lit walls.
ROWS = [("skirt-play", (330, 330, 1270, 900), "Default play camera"),
        ("skirt-close", (220, 160, 1600, 820), "Close three-quarter"),
        ("skirt-close", (300, 300, 1000, 580), "Close view, lit wall at 0.86x"),
        ("skirt-forest", (200, 250, 1600, 720), "Forest base wall (forest_base_skirt)")]
CW = 600
PAD, HEAD, LABEL = 16, 120, 30


def font(size, bold=False):
    for f in ("/System/Library/Fonts/Supplemental/Arial Bold.ttf" if bold else "/System/Library/Fonts/Supplemental/Arial.ttf",
              "/System/Library/Fonts/Helvetica.ttc"):
        try:
            return ImageFont.truetype(f, size)
        except OSError:
            continue
    return ImageFont.load_default()


cells = []
for key, box, _ in ROWS:
    row = []
    for col, _, _ in COLS:
        im = Image.open(SHOTS / f"{col}-{key}.png").convert("RGB").crop(box)
        row.append(im.resize((CW, round(CW * im.height / im.width)), Image.LANCZOS))
    cells.append(row)

heights = [r[0].height for r in cells]
W = PAD + len(COLS) * (CW + PAD) + 40
H = 70 + HEAD + sum(h + PAD + LABEL for h in heights) + 60
sheet = Image.new("RGB", (W, H), (24, 26, 30))
d = ImageDraw.Draw(sheet)
d.text((PAD + 40, 18), "Hex World tile skirt options (in engine, native, same scene and seed)", fill=(235, 235, 235), font=font(30, True))
y0 = 70
for i, (col, title, sub) in enumerate(COLS):
    x = PAD + 40 + i * (CW + PAD)
    d.text((x, y0 + 14), title, fill=(255, 255, 255) if col != "current" else (200, 200, 200), font=font(28, True))
    d.text((x, y0 + 56), sub, fill=(185, 185, 185), font=font(20))
y = y0 + HEAD
for (key, _, rlabel), row in zip(ROWS, cells):
    d.text((PAD, y + 4), rlabel, fill=(170, 170, 170), font=font(20))
    y += LABEL
    for i, im in enumerate(row):
        sheet.paste(im, (PAD + 40 + i * (CW + PAD), y))
    y += im.height + PAD
d.text((PAD + 40, y + 6), "The soil set is sampled by meadow_base_skirt (meadow, road, house tiles) and forest_base_skirt. Ocean has no strata "
       "skirt (water cut face); the beach wall is the water_terrain.wesl strata branch, unchanged in every column.",
       fill=(160, 160, 160), font=font(19))
sheet.save(OUT / "sheet.jpg", quality=88)
print("wrote", OUT / "sheet.jpg", sheet.size)
