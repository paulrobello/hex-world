"""Review sheet for the terrain-hump pass on the water-family bases (river_straight_base, river_bend_base,
river_spring_base; the beach seam for comparison). Takes the PNGs run.sh wrote for the `before` and
`after` tags, saves the shots the sheet uses as JPEGs (q85, 1024 px wide) beside this script, and
writes sheet.jpg: two contrast-stretched crops of the river row (BEFORE top, AFTER bottom), then
before/after pairs in the same poses. Dusk (`--kit_hooks dusk`) is the low sun from the south, the
grazing light that shows the ridge best. The crops come from the meadow pass's crop.py:
  python3 docs/checkpoints/terrain-hump/crop.py <before-dir>/before-hw-orbit-nograss-dusk.png \
    <after-dir>/after-hw-orbit-nograss-dusk.png docs/checkpoints/terrain-hump-water/crop-orbit-dusk.jpg 0 150 1600 650
(and hw-orbit-nograss into crop-orbit-noon.jpg).
Usage (repo root): python3 docs/checkpoints/terrain-hump-water/sheet.py <before-dir> <after-dir>"""
import pathlib
import sys

from PIL import Image, ImageDraw

DOCS = pathlib.Path(__file__).resolve().parent
before, after = pathlib.Path(sys.argv[1]), pathlib.Path(sys.argv[2])
PAIRS = [
    ("hw-orbit-nograss-dusk", "orbit, dusk, grass off: spring -> straight -> bend along the meadow field's north side"),
    ("hw-orbit-dusk", "orbit, dusk"),
    ("hw-orbit", "orbit, noon"),
    ("hw-low-nograss-dusk", "low oblique along the river row, dusk, grass off"),
    ("hw-seam-spring-dusk", "meadow / river_spring seam, dusk"),
    ("hw-seam-straight-dusk", "meadow / river_straight seam, dusk"),
    ("hw-seam-bend-dusk", "meadow / river_bend seam, dusk"),
    ("hw-seam-beach-dusk", "meadow / beach seam, dusk (beach grass is flat: no change there)"),
    ("hw-seam-straight", "meadow / river_straight seam, noon"),
    ("hw-seam-spring", "meadow / river_spring seam, noon"),
    ("hw-seam-beach", "meadow / beach seam, noon"),
    ("hw-eye-spring-dusk", "walk mode, eye level 1.7 m, dusk, toward the spring"),
    ("hw-eye-spring", "walk mode, eye level 1.7 m, noon, toward the spring"),
    ("hw-eye-straight-dusk", "walk mode, eye level 1.7 m, dusk, toward the straight river"),
    ("hw-eye-straight", "walk mode, eye level 1.7 m, noon, toward the straight river"),
    ("hw-eye-beach-dusk", "walk mode, eye level 1.7 m, dusk, toward the beach"),
    ("hw-eye-beach", "walk mode, eye level 1.7 m, noon, toward the beach"),
]


def jpeg(src, name):
    img = Image.open(src).convert("RGB")
    img.thumbnail((1024, 1024))
    out = DOCS / f"{name}.jpg"
    img.save(out, quality=85)
    return out


cells = [(DOCS / "crop-orbit-dusk.jpg", "orbit, dusk, grass off, river row, contrast-stretched crop: BEFORE top, AFTER bottom"),
         (DOCS / "crop-orbit-noon.jpg", "orbit, noon, grass off, river row, contrast-stretched crop: BEFORE top, AFTER bottom"),
         (DOCS / "diff-orbit-nograss-dusk.jpg", "what changed, orbit dusk (diffmap.py, x10; red brighter after, blue darker): "
                                               "only the river tiles' border bands"),
         (DOCS / "diff-seam-straight-dusk.jpg", "what changed, straight seam dusk (x10): the far border's sunlit lip darker, "
                                                "the near border's shaded lip brighter, so the ridge is gone")]
for shot, label in PAIRS:
    cells.append((jpeg(before / f"before-{shot}.png", f"before-{shot}"), f"BEFORE: {label}"))
    cells.append((jpeg(after / f"after-{shot}.png", f"after-{shot}"), f"AFTER: {label}"))
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
