"""bakery_shop_b back-left roof corner review sheet (owner A2, 2026-10-20): before (left) and after (right)
in play, straight down over the corner with the hex grid overlay (white line = tile border), the same view
zoomed 3x on the corner, an oblique close-up from the neighbouring meadow, and play distance without the
overlay; the title carries each LOD's measured distance to the border (border.json from run.sh).
Usage: python3 docs/checkpoints/bakery-corner/sheet.py (from the repo root; needs Pillow)."""
import json
import pathlib

from PIL import Image, ImageDraw

ROOT = pathlib.Path(__file__).resolve().parents[3]
DOCS = ROOT / "docs/checkpoints/bakery-corner"
W, H = 800, 500
ZOOM = 8  # the centre 200 x 125 px of the 1600 x 1000 shot, drawn at 4x its pixels
ROWS = [
    ("corner-top", None, "straight down over the corner, grid on"),
    ("corner-top", ZOOM, "the same, the corner at 4x its pixels"),
    ("corner-close", None, "oblique from the meadow across the border, grid on"),
    ("corner-play", None, "play distance ~40 m"),
]


def cell(path: pathlib.Path, zoom, label: str) -> Image.Image:
    im = Image.open(path).convert("RGB")
    if zoom:
        cw, ch = im.width // zoom, im.height // zoom
        x0, y0 = (im.width - cw) // 2, (im.height - ch) // 2
        im = im.crop((x0, y0, x0 + cw, y0 + ch)).resize((im.width, im.height), Image.NEAREST)
    im.thumbnail((W, H))
    c = Image.new("RGB", (W, H + 24), (24, 24, 24))
    c.paste(im, ((W - im.width) // 2, 24))
    ImageDraw.Draw(c).text((8, 5), label, fill=(235, 235, 235))
    return c


border = json.loads((DOCS / "border.json").read_text())


def lods(label: str, piece: str) -> str:
    r = border[label]
    return ", ".join(f"{k.rsplit('_', 1)[1]} {r[k]['borderM']:.3f}" for k in sorted(r) if k.startswith(piece))


lines = [
    "bakery_shop_b: back-left roof corner pulled in to 0.12 m from the hex border (owner A2, 2026-10-20). "
    "Before left, after right. Pending owner review.",
    f"distance to the border (m) before: {lods('before', 'bakery_shop_b')}",
    f"after: {lods('after', 'bakery_shop_b')}   (bakery_shop, the original: {lods('bakery_shop', 'bakery_shop')})",
    "Only LOD0's back-left eave corner moved: it slides 2.8 cm up its hip line (the eave course kick), from "
    f"{border['before']['bakery_shop_b_lod0']['blender']} to {border['after']['bakery_shop_b_lod0']['blender']} "
    "(Blender x, y, z). LOD1, LOD2 and the collider are unchanged.",
    "The top views look straight down on the corner (image centre); the white grid line at ground level is the "
    "tile border, and after the fix the roof corner sits about 10 px further from it on each axis.",
]
cells = []
for name, zoom, label in ROWS:
    cells.append(cell(DOCS / f"before-{name}.jpg", zoom, f"before: {label}"))
    cells.append(cell(DOCS / f"after-{name}.jpg", zoom, f"after: {label}"))
cw, ch = cells[0].size
top = 16 * len(lines) + 12
sheet = Image.new("RGB", (cw * 2, ch * len(ROWS) + top), (24, 24, 24))
d = ImageDraw.Draw(sheet)
for i, t in enumerate(lines):
    d.text((8, 6 + 16 * i), t, fill=(235, 235, 235))
for k, c in enumerate(cells):
    sheet.paste(c, ((k % 2) * cw, top + (k // 2) * ch))
sheet.save(DOCS / "sheet.jpg", quality=88)
print(f"wrote {DOCS / 'sheet.jpg'} ({sheet.width}x{sheet.height})")
