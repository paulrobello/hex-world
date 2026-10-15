"""Contact sheets for the prop variants from their committed six-view renders: one sheet per piece
(the six views, 3 x 2) and one per family (each piece's three-quarter view, original first).
Usage: python3 docs/checkpoints/prop-variants/sheets.py (from the repo root; needs Pillow)."""
import pathlib

from PIL import Image, ImageDraw

ROOT = pathlib.Path(__file__).resolve().parents[3]
OUT = ROOT / "docs/checkpoints/prop-variants"
FAMILIES = {
    "spruces": ["tree_forest", "tree_forest_b", "tree_forest_c"],
    "rocks": ["rock_small", "rock_small_b", "rock_small_c"],
    "clumps": ["wildflower_clump", "wildflower_clump_b", "wildflower_clump_c"],
    "singles": ["wildflower_single", "wildflower_single_b", "wildflower_single_c", "wildflower_single_d"],
}
VIEWS = ["front", "right", "back", "left", "top", "three_quarter"]
CELL = 340


def cell(path: pathlib.Path, label: str) -> Image.Image:
    im = Image.open(path).convert("RGB")
    im.thumbnail((CELL, CELL))
    c = Image.new("RGB", (CELL, CELL + 22), (24, 24, 24))
    c.paste(im, ((CELL - im.width) // 2, 22 + (CELL - im.height) // 2))
    ImageDraw.Draw(c).text((6, 4), label, fill=(230, 230, 230))
    return c


def grid(cells: list[Image.Image], cols: int) -> Image.Image:
    rows = (len(cells) + cols - 1) // cols
    w, h = cells[0].size
    g = Image.new("RGB", (w * cols, h * rows), (24, 24, 24))
    for k, c in enumerate(cells):
        g.paste(c, ((k % cols) * w, (k // cols) * h))
    return g


for fam, ids in FAMILIES.items():
    for pid in ids[1:] if fam != "singles" else ids:
        r = ROOT / "assets-source/kit" / pid / "renders"
        grid([cell(r / f"{v}.jpg", f"{pid} {v}") for v in VIEWS], 3).save(OUT / f"{pid}-sheet.jpg", quality=85)
    grid([cell(ROOT / "assets-source/kit" / p / "renders/three_quarter.jpg", p) for p in ids], len(ids)).save(
        OUT / f"family-{fam}.jpg", quality=85)
    print(fam)
