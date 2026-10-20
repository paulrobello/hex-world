"""Owner review sheet for the grazing sheep animal_sheep_c: the approved reference crops, the textured
EEVEE stills beside animal_sheep and animal_cow_b, the head at the grass, a top view and LOD2, and the
in-engine shots (play distance, sun and shade close-ups, the muzzle at the grass, 1.7 m eye level,
scale before the barn). Usage: python3 docs/checkpoints/animal_sheep_c/sheet.py (from the repo root,
after run.sh has written the JPEGs; needs Pillow)."""
import pathlib

from PIL import Image, ImageDraw

ROOT = pathlib.Path(__file__).resolve().parents[3]
DOCS = ROOT / "docs/checkpoints/animal_sheep_c"
REF = ROOT / "assets-source/refs/farm_animals"
CELL_W, CELL_H = 520, 390
# Reference crops (source pixels, 2400 x 1792).
REFS = [("ref-3: the flock grazing, heads down", "ref-3.jpg", (1350, 600, 2200, 1400)),
        ("ref-2 (approved; black face, creamy fleece)", "ref-2.jpg", (300, 250, 2200, 1600))]
STILLS = [("lineup", "EEVEE: animal_cow_b, animal_sheep, animal_sheep_c; 1.7 m figure"),
          ("animal_sheep_c-eevee-front_three_quarter", "EEVEE: animal_sheep_c three quarter"),
          ("animal_sheep_c-eevee-head", "EEVEE: animal_sheep_c muzzle at the grass"),
          ("animal_sheep_c-eevee-top", "EEVEE from above (as ref-3): animal_sheep left, animal_sheep_c right"),
          ("animal_sheep_c-eevee-lod2", "EEVEE: animal_sheep_c LOD2 (beyond 400 m)")]
ENGINE = [("sheepc-play", "engine: play distance, both farms (tilt off)"),
          ("sheepc-sun", "engine: grazing sheep, sun side"),
          ("sheepc-shade", "engine: grazing sheep, shade side"),
          ("sheepc-head", "engine: the muzzle at the grass"),
          ("sheepc-eye", "engine: eye level 1.7 m"),
          ("sheepc-scale-barn", "engine: the flock before the barn")]


def cell(im: Image.Image, label: str) -> Image.Image:
    im = im.convert("RGB")
    im.thumbnail((CELL_W, CELL_H))
    c = Image.new("RGB", (CELL_W, CELL_H + 24), (24, 24, 24))
    c.paste(im, ((CELL_W - im.width) // 2, 24 + (CELL_H - im.height) // 2))
    ImageDraw.Draw(c).text((8, 5), label, fill=(232, 232, 232))
    return c


cells = [cell(Image.open(REF / f).crop(box), label) for label, f, box in REFS]
cells += [cell(Image.open(DOCS / f"{n}.jpg"), label) for n, label in STILLS]
cells += [cell(Image.open(DOCS / f"{n}.jpg"), label) for n, label in ENGINE]
cols = 4
rows = (len(cells) + cols - 1) // cols
sheet = Image.new("RGB", (CELL_W * cols, (CELL_H + 24) * rows), (24, 24, 24))
for k, c in enumerate(cells):
    sheet.paste(c, ((k % cols) * CELL_W, (k // cols) * (CELL_H + 24)))
sheet.save(DOCS / "sheet.jpg", quality=85)
print("wrote", DOCS / "sheet.jpg")
