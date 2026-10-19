"""Owner review sheets for the farm animals, one per piece: the approved reference crops, the textured
EEVEE stills, the six-view three-quarter render, and the in-engine shots (play distance, sun and shade
close-ups, 1.7 m eye level, scale beside the oak and hay bales). Usage: python3
docs/checkpoints/farm_animals/sheet.py (from the repo root, after run.sh has written the JPEGs; needs
Pillow)."""
import pathlib

from PIL import Image, ImageDraw

ROOT = pathlib.Path(__file__).resolve().parents[3]
DOCS = ROOT / "docs/checkpoints/farm_animals"
REF = ROOT / "assets-source/refs/farm_animals"
CELL_W, CELL_H = 520, 390
# Reference crops (source pixels, 2400 x 1792): ref-1 cow, ref-2 sheep, ref-3's overhead herd.
CROPS = {
    "animal_cow": [("ref-1 (approved; A17: brown and white, hornless)", "ref-1.jpg", (100, 50, 2300, 1750)),
                   ("ref-3: overhead herd", "ref-3.jpg", (200, 300, 1300, 1550))],
    "animal_sheep": [("ref-2 (approved; black face, creamy fleece)", "ref-2.jpg", (300, 250, 2200, 1600)),
                     ("ref-3: overhead flock", "ref-3.jpg", (1300, 550, 2200, 1450))],
}
ENGINE = {
    "animal_cow": ["farm-play", "farm-cow-sun", "farm-cow-shade", "farm-eye", "farm-scale-oak"],
    "animal_sheep": ["farm-play", "farm-sheep-sun", "farm-sheep-shade", "farm-eye", "farm-scale-oak"],
}
LABELS = {
    "farm-play": "engine: play distance (tilt off)",
    "farm-cow-sun": "engine: close-up, sun side",
    "farm-cow-shade": "engine: close-up, shade side",
    "farm-sheep-sun": "engine: close-up, sun side",
    "farm-sheep-shade": "engine: close-up, shade side",
    "farm-eye": "engine: eye level 1.7 m",
    "farm-scale-oak": "engine: scale beside the oak and hay bales",
}


def cell(im: Image.Image, label: str) -> Image.Image:
    im = im.convert("RGB")
    im.thumbnail((CELL_W, CELL_H))
    c = Image.new("RGB", (CELL_W, CELL_H + 24), (24, 24, 24))
    c.paste(im, ((CELL_W - im.width) // 2, 24 + (CELL_H - im.height) // 2))
    ImageDraw.Draw(c).text((8, 5), label, fill=(232, 232, 232))
    return c


for piece in ("animal_cow", "animal_sheep"):
    cells = []
    for label, f, box in CROPS[piece]:
        cells.append(cell(Image.open(REF / f).crop(box), label))
    for v in ("left", "front_three_quarter", "back_three_quarter"):
        cells.append(cell(Image.open(DOCS / f"{piece}-eevee-{v}.jpg"), f"textured EEVEE: {v.replace('_', ' ')}"))
    cells.append(cell(Image.open(ROOT / "assets-source/kit" / piece / "renders/three_quarter.jpg"),
                      "six-view: three quarter (untextured)"))
    for n in ENGINE[piece]:
        cells.append(cell(Image.open(DOCS / f"{n}.jpg"), LABELS[n]))
    cols = 4
    rows = (len(cells) + cols - 1) // cols
    sheet = Image.new("RGB", (CELL_W * cols, (CELL_H + 24) * rows), (24, 24, 24))
    for k, c in enumerate(cells):
        sheet.paste(c, ((k % cols) * CELL_W, (k // cols) * (CELL_H + 24)))
    sheet.save(DOCS / f"{piece}-sheet.jpg", quality=85)
    print("wrote", DOCS / f"{piece}-sheet.jpg")
