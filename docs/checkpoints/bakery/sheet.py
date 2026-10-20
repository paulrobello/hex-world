"""Bakery review sheet: the approved reference, the bakery_pad six-view renders (top and three-quarter)
and the in-play shots from bakery-play.json (`make shots-bakery-play`, or the run.sh line in the
Makefile), tiled into docs/checkpoints/bakery/bakery-sheet.jpg with captions.

    uv run --with pillow==12.2.0 python docs/checkpoints/bakery/sheet.py
"""
import pathlib

from PIL import Image, ImageDraw

ROOT = pathlib.Path(__file__).resolve().parents[3]
OUT = ROOT / "docs/checkpoints/bakery/bakery-sheet.jpg"
CELLS = [
    ("assets-source/refs/bakery/ref-1.jpg", "approved ref-1 (A17)"),
    ("assets-source/kit/bakery_pad/renders/top.jpg", "bakery_pad top (6-view, untextured)"),
    ("assets-source/kit/bakery_pad/renders/three_quarter.jpg", "bakery_pad three-quarter"),
    ("docs/checkpoints/bakery/bakery-play.jpg", "play distance: house cluster, road, bakery"),
    ("docs/checkpoints/bakery/bakery-top.jpg", "the tile from above"),
    ("docs/checkpoints/bakery/bakery-cars-1.jpg", "visitors at the forecourt (t = 60 s)"),
    ("docs/checkpoints/bakery/bakery-cars-2.jpg", "visitors at the forecourt (t = 90 s)"),
    ("docs/checkpoints/bakery/bakery-close.jpg", "close-up of the forecourt and shop front"),
]
W, H, CAP = 640, 400, 26

sheet = Image.new("RGB", (W * 3, (H + CAP) * 3), (24, 24, 24))
draw = ImageDraw.Draw(sheet)
for i, (rel, cap) in enumerate(CELLS):
    path = ROOT / rel
    x, y = (i % 3) * W, (i // 3) * (H + CAP)
    if path.exists():
        im = Image.open(path).convert("RGB")
        im.thumbnail((W, H))
        sheet.paste(im, (x + (W - im.width) // 2, y + CAP + (H - im.height) // 2))
    else:
        cap += " (missing)"
    draw.text((x + 8, y + 6), cap, fill=(230, 230, 230))
sheet.save(OUT, quality=85)
print(f"wrote {OUT.relative_to(ROOT)}")
