"""Review sheet for the terrain-hump fix (card "Terrain caps: remove the hump along tile borders").
Takes the PNGs that run.sh wrote for the `before` and `after` tags, saves the shots the sheet uses as
JPEGs (q85, 1024 px wide) beside this script, and writes sheet.jpg: before/after pairs in the same
poses (dusk is the low sun, the grazing light that shows the ridge best), then the after-only
seams and the forest field. The two crops at the top come from crop.py on the same before/after pair:
  python3 docs/checkpoints/terrain-hump/crop.py <before-dir>/before-hump-orbit-dusk.png \
    <after-dir>/after-hump-orbit-dusk.png docs/checkpoints/terrain-hump/crop-orbit-dusk.jpg 300 450 1400 980
(and the same for hump-orbit into crop-orbit-noon.jpg). crop-dusk-shadows.jpg pairs the after dusk orbit with
the same pose from hump-play.json run with `--kit_hooks dusk --shadows off` (target/shots/hump-orbit.png).
Usage (repo root): python3 docs/checkpoints/terrain-hump/sheet.py <before-dir> <after-dir>"""
import pathlib
import sys

from PIL import Image, ImageDraw

DOCS = pathlib.Path(__file__).resolve().parent
before, after = pathlib.Path(sys.argv[1]), pathlib.Path(sys.argv[2])
PAIRS = [
    ("hump-orbit-dusk", "orbit, dusk (grazing light), 3x3 meadow"),
    ("hump-orbit", "orbit, noon, 3x3 meadow"),
    ("hump-low-dusk", "low oblique, dusk, meadow field"),
    ("hump-eye-dusk", "walk mode, eye level 1.7 m, dusk, across the (0,0)/(1,0) border"),
    ("hump-eye", "walk mode, eye level 1.7 m, noon"),
    ("hump-eye-b", "walk mode, eye level 1.7 m, noon, second pose"),
]
AFTER_ONLY = [
    ("hump-seam-forest-dusk", "after: meadow / forest seam, dusk"),
    ("hump-seam-road-dusk", "after: meadow / road seam, dusk"),
    ("hump-seam-beach-dusk", "after: meadow / beach seam, dusk"),
    ("hump-forest-dusk", "after: 3x3 forest field, dusk"),
    ("hump-seam-forest", "after: meadow / forest seam, noon"),
    ("hump-forest", "after: 3x3 forest field, noon"),
]


def jpeg(src, name):
    img = Image.open(src).convert("RGB")
    img.thumbnail((1024, 1024))
    out = DOCS / f"{name}.jpg"
    img.save(out, quality=85)
    return out


cells = [(DOCS / "crop-orbit-dusk.jpg", "orbit, dusk, contrast-stretched crop (crop.py): BEFORE top, AFTER bottom"),
         (DOCS / "crop-orbit-noon.jpg", "orbit, noon, contrast-stretched crop (crop.py): BEFORE top, AFTER bottom"),
         (DOCS / "crop-dusk-shadows.jpg", "residual, AFTER dusk crop: shadows on (top) vs --shadows off (bottom): "
                                          "the faint streaks are cast shadows of the remaining 0.11 m rise")]
for shot, label in PAIRS:
    cells.append((jpeg(before / f"before-{shot}.png", f"before-{shot}"), f"BEFORE: {label}"))
    cells.append((jpeg(after / f"after-{shot}.png", f"after-{shot}"), f"AFTER: {label}"))
for shot, label in AFTER_ONLY:
    cells.append((jpeg(after / f"after-{shot}.png", f"after-{shot}"), label))
W, H, PAD, LABEL = 800, 500, 12, 28
cols, rows = 2, (len(cells) + 1) // 2
sheet = Image.new("RGB", (cols * (W + PAD) + PAD, rows * (H + LABEL + PAD) + PAD), (30, 30, 30))
draw = ImageDraw.Draw(sheet)
for i, (path, label) in enumerate(cells):
    img = Image.open(path).convert("RGB")
    img.thumbnail((W, H))
    x = PAD + (i % cols) * (W + PAD)
    y = PAD + (i // cols) * (H + LABEL + PAD)
    sheet.paste(img, (x + (W - img.width) // 2, y + LABEL + (H - img.height) // 2))
    draw.text((x + 4, y + 6), label, fill=(230, 230, 230))
out = DOCS / "sheet.jpg"
sheet.save(out, quality=85)
print("wrote", out)
