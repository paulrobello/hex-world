"""Review sheet for the outcrop_granite redo (owner 2026-10-30: "the lines on it make it too
recognizable as stamped"): the approved reference and the in-engine shot that showed the straight T,
then before and after side by side from the same EEVEE views (redo_still.py).
Usage (from the repo root): python3 docs/checkpoints/outcrop_granite/redo_sheet.py <stills dir>"""
import pathlib
import sys

from PIL import Image, ImageDraw

ROOT = pathlib.Path(__file__).resolve().parents[3]
DOCS = ROOT / "docs/checkpoints/outcrop_granite"
stills = pathlib.Path(sys.argv[1])
VIEWS = [
    ("three_quarter", "three-quarter, 1.7 m figure at the right"),
    ("play", "play-like height and pitch"),
    ("top", "straight down"),
    ("close_sun", "close, sun side (about 6 m)"),
    ("close_shade", "close, shade side (about 6 m)"),
    ("eye", "eye level 1.7 m"),
]
ROWS = [[(ROOT / "assets-source/refs/boulders/ref-2.jpg", "ref-2 (approved A25): the whaleback outcrop"),
         (ROOT / "docs/checkpoints/boulders/b1-play-meadow.jpg", "in engine before (B1 review): the straight joint")]]
for name, label in VIEWS:
    ROWS.append([(stills / f"before-{name}.png", f"BEFORE: {label}"),
                 (stills / f"after-{name}.png", f"AFTER: {label}")])
W, H, PAD, LABEL = 800, 500, 12, 28
sheet = Image.new("RGB", (2 * (W + PAD) + PAD, len(ROWS) * (H + LABEL + PAD) + PAD), (30, 30, 30))
draw = ImageDraw.Draw(sheet)
for r, row in enumerate(ROWS):
    for c, (path, label) in enumerate(row):
        img = Image.open(path).convert("RGB")
        img.thumbnail((W, H))
        x = PAD + c * (W + PAD)
        y = PAD + r * (H + LABEL + PAD)
        sheet.paste(img, (x + (W - img.width) // 2, y + LABEL + (H - img.height) // 2))
        draw.text((x + 4, y + 6), label, fill=(230, 230, 230))
sheet.save(DOCS / "redo-sheet.jpg", quality=85)
print("wrote", DOCS / "redo-sheet.jpg")
