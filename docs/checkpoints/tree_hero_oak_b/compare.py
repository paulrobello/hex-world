"""Six-view comparison for the review sheet: rows front, left, three-quarter and top; columns
tree_hero_oak, tree_hero_oak_b, tree_oak and tree_oak_b (untextured renders from make kit-renders).
Writes docs/checkpoints/tree_hero_oak_b/renders-compare.jpg. Usage: python3 docs/checkpoints/tree_hero_oak_b/compare.py"""
import pathlib

from PIL import Image

K = pathlib.Path("assets-source/kit")
PIECES = ["tree_hero_oak", "tree_hero_oak_b", "tree_oak", "tree_oak_b"]
VIEWS = ["front", "left", "three_quarter", "top"]
W = 300
rows = []
for v in VIEWS:
    ims = [Image.open(K / p / "renders" / f"{v}.jpg").convert("RGB") for p in PIECES]
    ims = [im.resize((W, int(im.height * W / im.width))) for im in ims]
    row = Image.new("RGB", (W * len(ims), max(im.height for im in ims)), "white")
    for i, im in enumerate(ims):
        row.paste(im, (i * W, 0))
    rows.append(row)
out = Image.new("RGB", (rows[0].width, sum(r.height for r in rows)), "white")
y = 0
for r in rows:
    out.paste(r, (0, y))
    y += r.height
out.save("docs/checkpoints/tree_hero_oak_b/renders-compare.jpg", quality=85)
print("wrote renders-compare.jpg", out.size)
