"""Review sheet for the terrain flatten (owner D6-Q1, 2026-10-20: flatten the border rise in geometry).
Takes the PNGs run.sh wrote for the `before` and `after` tags, saves the shots the sheet uses as JPEGs
(q85, 1024 px wide) beside this script, writes contrast-stretched before/after crops of the dusk
orbits (the terrain-hump crop.py window), and lays out sheet.jpg: the crops, the height profiles across
a border (profile.txt, from profile.py), then before/after pairs in the same poses, dusk (the sun
about 3 degrees up) and noon.
Usage (repo root): python3 docs/checkpoints/terrain-flatten/sheet.py <before-dir> <after-dir>"""
import pathlib
import subprocess
import sys

from PIL import Image, ImageDraw

DOCS = pathlib.Path(__file__).resolve().parent
CROP = DOCS.parent / "terrain-hump" / "crop.py"
before, after = pathlib.Path(sys.argv[1]), pathlib.Path(sys.argv[2])
PAIRS = [
    ("hump-orbit-dusk", "orbit, dusk, 3x3 meadow (terrain-hump camera)"),
    ("hump-orbit", "orbit, noon, 3x3 meadow"),
    ("hump-low-nograss-dusk", "low oblique, dusk, no grass, meadow field"),
    ("hump-forest-dusk", "3x3 forest field, dusk"),
    ("hump-seam-forest-dusk", "meadow / forest seam, dusk"),
    ("hump-eye-dusk", "walk mode, eye level 1.7 m, dusk, across a meadow border"),
    ("hw-orbit-dusk", "orbit, dusk: spring, river straight and bend among meadows"),
    ("hw-seam-straight-dusk", "river straight / meadow seam, dusk"),
    ("hw-seam-spring-dusk", "spring / meadow seam, dusk"),
    ("fw-orbit", "farm and wheat field tiles, noon"),
    ("fw-orbit-dusk", "farm and wheat field tiles, dusk"),
    ("fw-paddock-dusk", "paddock fence on the meadow, dusk"),
    ("fw-hay", "hay bales at the farm's slots, noon"),
    ("fw-wheat-edge-dusk", "wheat field bank on the meadow, dusk"),
]


def jpeg(src, name):
    img = Image.open(src).convert("RGB")
    img.thumbnail((1024, 1024))
    out = DOCS / f"{name}.jpg"
    img.save(out, quality=85)
    return out


for shot, window in (("hump-orbit-dusk", "300 450 1400 980"), ("hw-orbit-dusk", "300 450 1400 980")):
    subprocess.run([sys.executable, str(CROP), str(before / f"before-{shot}.png"), str(after / f"after-{shot}.png"),
                    str(DOCS / f"crop-{shot}.jpg"), *window.split()], check=True)
cells = [(DOCS / "crop-hump-orbit-dusk.jpg", "meadow orbit, dusk, contrast-stretched crop: BEFORE top, AFTER bottom"),
         (DOCS / "crop-hw-orbit-dusk.jpg", "river orbit, dusk, contrast-stretched crop: BEFORE top, AFTER bottom"),
         ("TEXT", "height profiles (profile.py)")]
for shot, label in PAIRS:
    cells.append((jpeg(before / f"before-{shot}.png", f"before-{shot}"), f"BEFORE: {label}"))
    cells.append((jpeg(after / f"after-{shot}.png", f"after-{shot}"), f"AFTER: {label}"))
W, H, PAD, LABEL = 800, 500, 12, 28
cols = 2
# The profile text takes a full row.
layout, row, col = [], 0, 0
for path, label in cells:
    if path == "TEXT":
        if col:
            row, col = row + 1, 0
        layout.append((path, label, row, 0, 2))
        row += 1
        continue
    layout.append((path, label, row, col, 1))
    col += 1
    if col == cols:
        row, col = row + 1, 0
rows = row + (1 if col else 0)
sheet = Image.new("RGB", (cols * (W + PAD) + PAD, rows * (H + LABEL + PAD) + PAD), (30, 30, 30))
draw = ImageDraw.Draw(sheet)
text = (DOCS / "profile.txt").read_text().splitlines()
for path, label, r, c, span in layout:
    x, y = PAD + c * (W + PAD), PAD + r * (H + LABEL + PAD)
    draw.text((x + 4, y + 6), label, fill=(230, 230, 230))
    if path == "TEXT":
        for i, line in enumerate(text[:34]):
            draw.text((x + 8, y + LABEL + 6 + i * 14), line, fill=(220, 220, 200))
        continue
    img = Image.open(path).convert("RGB")
    img.thumbnail((W, H))
    sheet.paste(img, (x + (W - img.width) // 2, y + LABEL + (H - img.height) // 2))
out = DOCS / "sheet.jpg"
sheet.save(out, quality=85)
print("wrote", out)
