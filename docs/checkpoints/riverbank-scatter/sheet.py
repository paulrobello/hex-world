"""Builds the riverbank scatter review sheet (sheet.jpg here): the five new bank props' three-quarter and
top renders (assets-source/kit/<id>/renders/), then the river stretch before and after at the
stretch, close and along cameras (rs-<before|after>-<camera>.jpg, from run.sh).
Usage (from the repo root): python3 docs/checkpoints/riverbank-scatter/sheet.py"""

import pathlib

from PIL import Image, ImageDraw

here = pathlib.Path(__file__).resolve().parent
repo = here.parents[2]
PIECES = ("river_stones", "river_stones_b", "river_stones_c", "river_reeds", "river_reeds_b")
W = 2400
tile = W // len(PIECES)
rows = []
for view in ("three_quarter", "top"):
    row = Image.new("RGB", (W, tile), (40, 40, 40))
    for i, pid in enumerate(PIECES):
        im = Image.open(repo / "assets-source/kit" / pid / "renders" / f"{view}.jpg").convert("RGB")
        im = im.resize((tile, tile))
        ImageDraw.Draw(im).text((10, 10), f"{pid} ({view})", fill=(255, 255, 255))
        row.paste(im, (i * tile, 0))
    rows.append(row)
half = W // 2
for cam in ("stretch", "close", "along"):
    row = Image.new("RGB", (W, half * 10 // 16), (0, 0, 0))
    for i, which in enumerate(("before", "after")):
        im = Image.open(here / f"rs-{which}-{cam}.jpg").convert("RGB").resize((half, half * 10 // 16))
        ImageDraw.Draw(im).text((10, 10), f"{which}: {cam}", fill=(255, 255, 255))
        row.paste(im, (i * half, 0))
    rows.append(row)

def pair(left, right, labels):
    row = Image.new("RGB", (W, half * 10 // 16), (0, 0, 0))
    for i, (path, label) in enumerate(zip((left, right), labels)):
        im = Image.open(path).convert("RGB")
        im.thumbnail((half, half * 10 // 16))
        ImageDraw.Draw(im).text((10, 10), label, fill=(255, 255, 255))
        row.paste(im, (i * half + (half - im.width) // 2, 0))
    return row


rows.append(pair(repo / "assets-source/refs/river/ref-1.jpg", here / "rs-after-eye.jpg",
                 ("approved reference (assets-source/refs/river/ref-1.jpg)", "after: eye level, 1.7 m on the bank")))
fam = ["bend", "mouth", "straight", "spring", "mouth-l", "bend-l"]
third = W // 3
for k in range(0, len(fam), 3):
    row = Image.new("RGB", (W, third * 10 // 16), (0, 0, 0))
    for i, name in enumerate(fam[k:k + 3]):
        im = Image.open(here / f"rs-after-family-{name}.jpg").convert("RGB").resize((third, third * 10 // 16))
        ImageDraw.Draw(im).text((10, 10), f"after: {name} (mouth world)", fill=(255, 255, 255))
        row.paste(im, (i * third, 0))
    rows.append(row)
sheet = Image.new("RGB", (W, sum(r.height for r in rows)), (0, 0, 0))
y = 0
for r in rows:
    sheet.paste(r, (0, y))
    y += r.height
sheet.save(here / "sheet.jpg", quality=85)
print(f"wrote {here / 'sheet.jpg'} {sheet.size}")
