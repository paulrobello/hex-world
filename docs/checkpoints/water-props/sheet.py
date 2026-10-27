"""B3 water-props review sheet: the approved references, the headless EEVEE stills and the engine
shots, captioned, into docs/checkpoints/water-props/sheet.jpg.
Usage: python3 docs/checkpoints/water-props/sheet.py <eevee still dir>
(the stills from still.py; the engine shots are the JPEGs make shots-piece wrote beside this file)."""
import pathlib
import sys

from PIL import Image, ImageDraw, ImageFont

HERE = pathlib.Path(__file__).resolve().parent
ROOT = HERE.parents[2]
stills = pathlib.Path(sys.argv[1])
CELL_W, CELL_H, CAP = 640, 400, 34
TILES = [
    (ROOT / "assets-source/refs/water_props/ref-1.jpg", "approved ref-1: pads (colour, form)"),
    (ROOT / "assets-source/refs/water_props/ref-2.jpg", "approved ref-2: reed bed"),
    (stills / "lineup.png", "EEVEE still: all four, 1.7 m figure"),
    (stills / "lily_cluster_b-eevee.png", "EEVEE: lily_cluster_b (white lily)"),
    (stills / "lily_cluster_c-eevee.png", "EEVEE: lily_cluster_c (pink bud)"),
    (stills / "reed_bed-eevee.png", "EEVEE: reed_bed"),
    (HERE / "wp-play-spring.jpg", "engine: spring at play distance"),
    (HERE / "wp-play-bend-a.jpg", "engine: river bend at play distance"),
    (HERE / "wp-close-bend-a.jpg", "engine: close-up, bend"),
    (HERE / "wp-close-spring.jpg", "engine: close-up, spring"),
    (HERE / "wp-eye-bend-a.jpg", "engine: eye level 1.7 m, bend"),
    (HERE / "wp-pads-side-by-side.jpg", "engine: spring's built-in pads beside lily_cluster_c"),
]
COLS = 3
tiles = [(p, c) for p, c in TILES if p.exists()]
rows = (len(tiles) + COLS - 1) // COLS
sheet = Image.new("RGB", (COLS * CELL_W, rows * (CELL_H + CAP)), (24, 24, 26))
draw = ImageDraw.Draw(sheet)
try:
    font = ImageFont.truetype("/System/Library/Fonts/Supplemental/Arial.ttf", 20)
except OSError:
    font = ImageFont.load_default()
for i, (p, cap) in enumerate(tiles):
    im = Image.open(p).convert("RGB")
    im.thumbnail((CELL_W, CELL_H))
    x, y = (i % COLS) * CELL_W, (i // COLS) * (CELL_H + CAP)
    sheet.paste(im, (x + (CELL_W - im.width) // 2, y + (CELL_H - im.height) // 2))
    draw.text((x + 10, y + CELL_H + 6), cap, fill=(230, 230, 230), font=font)
missing = [str(p.relative_to(ROOT)) for p, _ in TILES if not p.exists()]
sheet.save(HERE / "sheet.jpg", quality=85)
print("sheet", HERE / "sheet.jpg", f"{len(tiles)} tiles", "missing:", missing)
