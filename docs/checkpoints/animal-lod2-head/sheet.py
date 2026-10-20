"""Before/after review sheet for the LOD2 head fix: each row is one in-engine camera (shots.json) with
the debug Force LOD set, the committed GLBs before the fix on the left and the rebuilt ones on the
right. The LOD2 rows show animal_cow's and animal_sheep's heads, which drew a beak on three stations
and draw four now; animal_cow_b and animal_sheep_c already drew four (unchanged, for reference). The
LOD1 and LOD0 rows show the same cameras at the nearer LODs, which the fix leaves vertex-identical.
Usage: python3 docs/checkpoints/animal-lod2-head/sheet.py (from the repo root, after run.py before and
run.py after; needs Pillow). Writes docs/checkpoints/animal-lod2-head/sheet.jpg."""
import pathlib

from PIL import Image, ImageDraw

ROOT = pathlib.Path(__file__).resolve().parents[3]
SRC = ROOT / "target/lod2head"
DOCS = ROOT / "docs/checkpoints/animal-lod2-head"
CELL_W, CELL_H, LABEL = 800, 500, 26
ROWS = [("cowside-lod2", "animal_cow head, side, Force LOD 2"),
        ("cowfront-lod2", "animal_cow head, front three-quarter, Force LOD 2"),
        ("cow-lod2", "animal_cow, the neck-fix sheet's camera, Force LOD 2"),
        ("sheepside-lod2", "animal_sheep heads, side, Force LOD 2"),
        ("sheepfront-lod2", "animal_sheep head, front, Force LOD 2"),
        ("sheep-lod2", "both animal_sheep ewes, the neck-fix sheet's camera, Force LOD 2"),
        ("cowb-lod2", "animal_cow_b (already four head stations, unchanged), Force LOD 2"),
        ("sheepc-lod2", "animal_sheep_c (already four head stations, unchanged), Force LOD 2"),
        ("cowside-lod1", "animal_cow head, side, Force LOD 1"),
        ("cowfront-lod1", "animal_cow head, front three-quarter, Force LOD 1"),
        ("sheepside-lod1", "animal_sheep heads, side, Force LOD 1"),
        ("sheepfront-lod1", "animal_sheep head, front, Force LOD 1"),
        ("cowside-lod0", "animal_cow head, side, Force LOD 0"),
        ("cowfront-lod0", "animal_cow head, front three-quarter, Force LOD 0"),
        ("sheepside-lod0", "animal_sheep heads, side, Force LOD 0"),
        ("sheepfront-lod0", "animal_sheep head, front, Force LOD 0")]


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
        c = cell(SRC / side / f"lod2head-{name}.png", f"{side.upper()}: {label}")
        sheet.paste(c, (k * (CELL_W + 8), r * (CELL_H + LABEL)))
sheet.save(DOCS / "sheet.jpg", quality=85)
print("wrote", DOCS / "sheet.jpg")
