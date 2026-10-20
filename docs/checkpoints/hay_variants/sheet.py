"""Owner review sheet for the hay variants (hay_bale_round_b, hay_bale_stack_b): the approved reference
crops, the textured EEVEE stills beside the originals and alone, the six-view three-quarter renders,
and the in-engine shots on the hay-variants farm (play distance with the originals' farm for
comparison, sun and shade close-ups of each, 1.7 m eye level in walk mode, scale beside the barn).
Usage: python3 docs/checkpoints/hay_variants/sheet.py (from the repo root, after run.sh; needs Pillow)."""
import pathlib

from PIL import Image, ImageDraw

ROOT = pathlib.Path(__file__).resolve().parents[3]
DOCS = ROOT / "docs/checkpoints/hay_variants"
REF = ROOT / "assets-source/refs/farm"
CELL_W, CELL_H = 520, 390
CELLS = [
    ("ref", "ref-1: round bales and square-bale stacks", REF / "ref-1.jpg", (950, 450, 1980, 760)),
    ("ref", "ref-2: round and square bales, loose straw", REF / "ref-2.jpg", (1150, 650, 1780, 1000)),
    ("img", "EEVEE: hay_bale_round (left) and _b", DOCS / "hay-round-pair.jpg", None),
    ("img", "EEVEE: hay_bale_stack (left) and _b", DOCS / "hay-stack-pair.jpg", None),
    ("img", "EEVEE: round_b three quarter", DOCS / "hay_bale_round_b-eevee-three_quarter.jpg", None),
    ("img", "EEVEE: round_b low side", DOCS / "hay_bale_round_b-eevee-low_side.jpg", None),
    ("img", "EEVEE: stack_b three quarter", DOCS / "hay_bale_stack_b-eevee-three_quarter.jpg", None),
    ("img", "EEVEE: stack_b low side", DOCS / "hay_bale_stack_b-eevee-low_side.jpg", None),
    ("img", "six-view: round_b (untextured)", ROOT / "assets-source/kit/hay_bale_round_b/renders/three_quarter.jpg", None),
    ("img", "six-view: stack_b (untextured)", ROOT / "assets-source/kit/hay_bale_stack_b/renders/three_quarter.jpg", None),
    ("img", "engine: play distance, variants (tilt off)", DOCS / "hayv-play.jpg", None),
    ("img", "engine: play distance, the farm's originals", DOCS / "hayv-play-originals.jpg", None),
    ("img", "engine: stack_b close-up, sun side", DOCS / "hayv-stack-sun.jpg", None),
    ("img", "engine: stack_b close-up, shade side", DOCS / "hayv-stack-shade.jpg", None),
    ("img", "engine: round_b close-up, sun side", DOCS / "hayv-round-sun.jpg", None),
    ("img", "engine: round_b close-up, shade side", DOCS / "hayv-round-shade.jpg", None),
    ("img", "engine: walk mode, eye level 1.7 m", DOCS / "hayv-walk-eye.jpg", None),
    ("img", "engine: scale beside the barn", DOCS / "hayv-scale-barn.jpg", None),
]


def cell(im: Image.Image, label: str) -> Image.Image:
    im = im.convert("RGB")
    im.thumbnail((CELL_W, CELL_H))
    c = Image.new("RGB", (CELL_W, CELL_H + 24), (24, 24, 24))
    c.paste(im, ((CELL_W - im.width) // 2, 24 + (CELL_H - im.height) // 2))
    ImageDraw.Draw(c).text((8, 5), label, fill=(232, 232, 232))
    return c


cells = []
for kind, label, path, box in CELLS:
    im = Image.open(path)
    cells.append(cell(im.crop(box) if box else im, label))
cols = 4
rows = (len(cells) + cols - 1) // cols
sheet = Image.new("RGB", (CELL_W * cols, (CELL_H + 24) * rows), (24, 24, 24))
for k, c in enumerate(cells):
    sheet.paste(c, ((k % cols) * CELL_W, (k // cols) * (CELL_H + 24)))
sheet.save(DOCS / "sheet.jpg", quality=85)
print("wrote", DOCS / "sheet.jpg")
