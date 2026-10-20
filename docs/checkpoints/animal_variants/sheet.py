"""Owner review sheet for the farm animal variants animal_cow_b (grazing) and animal_sheep_b (lamb): the
approved reference crops, the textured EEVEE stills beside the originals and at LOD2, and the in-engine
shots (play distance, sun and shade close-ups of each, 1.7 m eye level, scale before the barn). Usage:
python3 docs/checkpoints/animal_variants/sheet.py (from the repo root, after run.sh has written the
JPEGs; needs Pillow)."""
import pathlib

from PIL import Image, ImageDraw

ROOT = pathlib.Path(__file__).resolve().parents[3]
DOCS = ROOT / "docs/checkpoints/animal_variants"
REF = ROOT / "assets-source/refs/farm_animals"
CELL_W, CELL_H = 520, 390
# Reference crops (source pixels, 2400 x 1792).
REFS = [("ref-1 (approved; A17: brown and white, hornless)", "ref-1.jpg", (100, 50, 2300, 1750)),
        ("ref-3: the herd, two cows grazing", "ref-3.jpg", (200, 300, 1300, 1550)),
        ("ref-2 (approved; black face, creamy fleece)", "ref-2.jpg", (300, 250, 2200, 1600))]
STILLS = [("cows-lineup", "EEVEE: animal_cow (behind) and animal_cow_b, 1.7 m figure"),
          ("animal_cow_b-eevee-front_three_quarter", "EEVEE: animal_cow_b three quarter"),
          ("animal_cow_b-eevee-head", "EEVEE: animal_cow_b muzzle at the grass"),
          ("animal_cow_b-eevee-lod2", "EEVEE: animal_cow_b LOD2 (beyond 400 m)"),
          ("sheep-lineup", "EEVEE: animal_sheep (behind) and the lamb, 1.7 m figure"),
          ("animal_sheep_b-eevee-front_three_quarter", "EEVEE: animal_sheep_b three quarter"),
          ("animal_sheep_b-eevee-head", "EEVEE: animal_sheep_b head"),
          ("animal_sheep_b-eevee-lod2", "EEVEE: animal_sheep_b LOD2 (beyond 400 m)")]
ENGINE = [("var-play", "engine: play distance, both farms (tilt off)"),
          ("var-cow-sun", "engine: grazing cow, sun side"),
          ("var-cow-shade", "engine: grazing cow and lamb, shade side"),
          ("var-lamb-sun", "engine: lamb, sun side"),
          ("var-lamb-shade", "engine: lamb, shade side"),
          ("var-eye", "engine: eye level 1.7 m"),
          ("var-scale-barn", "engine: scale before the barn")]


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
