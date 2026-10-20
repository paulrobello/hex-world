"""Owner review sheet for the Hereford-coat cow animal_cow_c: the approved reference crops, the textured
EEVEE stills (the three cows side by side, three quarter front and back, the face, a top view beside
animal_cow, LOD2) and the in-engine play shots on a farm whose cow slot draws the variant (play
distance over it and a neighbour farm keeping animal_cow, a nearer view, sun and shade close-ups, the
face, scale before the barn, 1.7 m eye level in walk mode).
Usage: python3 docs/checkpoints/animal_cow_c/sheet.py (from the repo root, after run.sh has written the
JPEGs; needs Pillow)."""
import pathlib

from PIL import Image, ImageDraw

ROOT = pathlib.Path(__file__).resolve().parents[3]
DOCS = ROOT / "docs/checkpoints/animal_cow_c"
REF = ROOT / "assets-source/refs/farm_animals"
CELL_W, CELL_H = 520, 390
# Reference crops (source pixels).
REFS = [("ref-1 (approved; A17: brown and white, hornless)", "ref-1.jpg", None),
        ("ref-3: overhead herd", "ref-3.jpg", None)]
STILLS = [("lineup", "EEVEE: animal_cow_c, animal_cow_b, animal_cow; 1.7 m figure"),
          ("animal_cow_c-eevee-front_three_quarter", "EEVEE: animal_cow_c three quarter front"),
          ("animal_cow_c-eevee-back_three_quarter", "EEVEE: animal_cow_c three quarter back"),
          ("animal_cow_c-eevee-face", "EEVEE: the white face, red eye patches and ears"),
          ("animal_cow_c-eevee-top", "EEVEE from above (as ref-3): animal_cow left, animal_cow_c right"),
          ("animal_cow_c-eevee-lod2", "EEVEE: animal_cow_c LOD2 (beyond 400 m)")]
ENGINE = [("cowc-play", "engine: play distance; (1, -2) draws animal_cow_c, (1, -1) keeps animal_cow"),
          ("cowc-play-near", "engine: nearer play view of the variant's farm"),
          ("cowc-sun", "engine: close-up, sun side"),
          ("cowc-shade", "engine: close-up, shade side"),
          ("cowc-face", "engine: the face"),
          ("cowc-scale-barn", "engine: scale before the barn"),
          ("cowc-walk-eye", "engine: walk mode, eye level 1.7 m")]


def cell(im: Image.Image, label: str) -> Image.Image:
    im = im.convert("RGB")
    im.thumbnail((CELL_W, CELL_H))
    c = Image.new("RGB", (CELL_W, CELL_H + 24), (24, 24, 24))
    c.paste(im, ((CELL_W - im.width) // 2, 24 + (CELL_H - im.height) // 2))
    ImageDraw.Draw(c).text((8, 5), label, fill=(232, 232, 232))
    return c


def ref(f: str, box):
    im = Image.open(REF / f)
    return im.crop(box) if box else im


cells = [cell(ref(f, box), label) for label, f, box in REFS]
cells += [cell(Image.open(DOCS / f"{n}.jpg"), label) for n, label in STILLS]
cells += [cell(Image.open(DOCS / f"{n}.jpg"), label) for n, label in ENGINE]
cols = 4
rows = (len(cells) + cols - 1) // cols
sheet = Image.new("RGB", (CELL_W * cols, (CELL_H + 24) * rows), (24, 24, 24))
for k, c in enumerate(cells):
    sheet.paste(c, ((k % cols) * CELL_W, (k // cols) * (CELL_H + 24)))
sheet.save(DOCS / "sheet.jpg", quality=85)
print("wrote", DOCS / "sheet.jpg")
