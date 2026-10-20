"""Before/after review sheet for the LOD2 neck fix: each row is one in-engine camera (shots.json) with
the debug Force LOD set, the committed maps before the fix on the left and the rebuilt maps on the
right. LOD2 rows show the junction patches the fix removes; the LOD1 and LOD0 rows show the same
cameras at the nearer LODs (LOD0 should read the same).
Usage: python3 docs/checkpoints/animal-lod2-neck/sheet.py (from the repo root, after run.py before and
run.py after; needs Pillow). Writes docs/checkpoints/animal-lod2-neck/sheet.jpg."""
import pathlib

from PIL import Image, ImageDraw

ROOT = pathlib.Path(__file__).resolve().parents[3]
SRC = ROOT / "target/lod2neck"
DOCS = ROOT / "docs/checkpoints/animal-lod2-neck"
CELL_W, CELL_H, LABEL = 800, 500, 26
ROWS = [("cowb-lod2", "animal_cow_b (grazing cow), Force LOD 2"),
        ("sheepc-lod2", "animal_sheep_c (grazing sheep), Force LOD 2"),
        ("sheepb-lod2", "animal_sheep_b (lamb), Force LOD 2"),
        ("sheep-lod2", "animal_sheep, Force LOD 2"),
        ("cow-lod2", "animal_cow, Force LOD 2"),
        ("nw-lod2", "the paddock from the north-west, Force LOD 2"),
        ("cowb-lod1", "animal_cow_b, Force LOD 1"),
        ("sheepc-lod1", "animal_sheep_c, Force LOD 1"),
        ("cowb-lod0", "animal_cow_b, Force LOD 0"),
        ("sheepc-lod0", "animal_sheep_c, Force LOD 0")]


def cell(path: pathlib.Path, label: str) -> Image.Image:
    im = Image.open(path).convert("RGB")
    im.thumbnail((CELL_W, CELL_H))
    c = Image.new("RGB", (CELL_W, CELL_H + LABEL), (24, 24, 24))
    c.paste(im, ((CELL_W - im.width) // 2, LABEL))
    ImageDraw.Draw(c).text((8, 7), label, fill=(232, 232, 232))
    return c


sheet = Image.new("RGB", (CELL_W * 2 + 8, (CELL_H + LABEL) * len(ROWS)), (24, 24, 24))
for r, (name, label) in enumerate(ROWS):
    for k, side in enumerate(("before", "after")):
        c = cell(SRC / side / f"lod2neck-{name}.png", f"{side.upper()}: {label}")
        sheet.paste(c, (k * (CELL_W + 8), r * (CELL_H + LABEL)))
sheet.save(DOCS / "sheet.jpg", quality=85)
print("wrote", DOCS / "sheet.jpg")
