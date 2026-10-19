"""Owner review sheets for the hay bales, one per piece: the approved reference crop, the textured
EEVEE stills, the six-view three-quarter render, and the in-engine shots (play distance, sun and shade
close-ups, 1.7 m eye level, scale beside the oak). Usage: python3 docs/checkpoints/hay_bales/sheet.py
(from the repo root, after run.sh has written the JPEGs; needs Pillow)."""
import pathlib

from PIL import Image, ImageDraw

ROOT = pathlib.Path(__file__).resolve().parents[3]
DOCS = ROOT / "docs/checkpoints/hay_bales"
REF = ROOT / "assets-source/refs/farm"
CELL_W, CELL_H = 520, 390
# Reference crops (source pixels): ref-2's yard bales, ref-1's barn-side bales.
CROPS = {
    "hay_bale_round": [("ref-2: round bale (left)", "ref-2.jpg", (250, 880, 560, 1130)),
                       ("ref-2: round and square bales", "ref-2.jpg", (1150, 650, 1780, 1000))],
    "hay_bale_stack": [("ref-1: square bale stacks", "ref-1.jpg", (1520, 560, 1960, 740)),
                       ("ref-2: round and square bales", "ref-2.jpg", (1150, 650, 1780, 1000))],
}
ENGINE = {
    "hay_bale_round": ["hay-play", "hay-round-sun", "hay-round-shade", "hay-eye", "hay-scale-oak"],
    "hay_bale_stack": ["hay-play", "hay-stack-sun", "hay-stack-shade", "hay-eye", "hay-scale-oak"],
}
LABELS = {
    "hay-play": "engine: play distance (tilt off)",
    "hay-round-sun": "engine: close-up, sun side",
    "hay-round-shade": "engine: close-up, shade side",
    "hay-stack-sun": "engine: close-up, sun side",
    "hay-stack-shade": "engine: close-up, shade side",
    "hay-eye": "engine: eye level 1.7 m",
    "hay-scale-oak": "engine: scale beside the hero oak",
}


def cell(im: Image.Image, label: str) -> Image.Image:
    im = im.convert("RGB")
    im.thumbnail((CELL_W, CELL_H))
    c = Image.new("RGB", (CELL_W, CELL_H + 24), (24, 24, 24))
    c.paste(im, ((CELL_W - im.width) // 2, 24 + (CELL_H - im.height) // 2))
    ImageDraw.Draw(c).text((8, 5), label, fill=(232, 232, 232))
    return c


for piece in ("hay_bale_round", "hay_bale_stack"):
    cells = []
    for label, f, box in CROPS[piece]:
        cells.append(cell(Image.open(REF / f).crop(box), label))
    for v in ("three_quarter", "low_side"):
        cells.append(cell(Image.open(DOCS / f"{piece}-eevee-{v}.jpg"), f"textured EEVEE: {v.replace('_', ' ')}"))
    cells.append(cell(Image.open(ROOT / "assets-source/kit" / piece / "renders/three_quarter.jpg"), "six-view: three quarter (untextured)"))
    for n in ENGINE[piece]:
        cells.append(cell(Image.open(DOCS / f"{n}.jpg"), LABELS[n]))
    cols = 4
    rows = (len(cells) + cols - 1) // cols
    sheet = Image.new("RGB", (CELL_W * cols, (CELL_H + 24) * rows), (24, 24, 24))
    for k, c in enumerate(cells):
        sheet.paste(c, ((k % cols) * CELL_W, (k // cols) * (CELL_H + 24)))
    sheet.save(DOCS / f"{piece}-sheet.jpg", quality=85)
    print("wrote", DOCS / f"{piece}-sheet.jpg")
