"""bakery_shop_b owner review sheet (SKILL.md "Owner checkpoints"): the approved references, the textured
headless EEVEE stills by day and night (evidence only), and the in-engine shots with tilt-shift off: the
kit scenario (play distance and straight above beside bakery_shop, sun and shade close-ups, the scale shot
down the road with house_cluster) and the play scenario with the variant drawn in a bakery slot (play
distance, sun and shade close-ups, the pair, eye level 1.7 m in walk mode). A prop has no border seam:
the play shots show the bakery tile meeting its neighbours, unchanged. 3 columns.
Usage: python3 docs/checkpoints/bakery_shop_b/sheet.py (from the repo root; needs Pillow)."""
import json
import pathlib

from PIL import Image, ImageDraw

ROOT = pathlib.Path(__file__).resolve().parents[3]
DOCS = ROOT / "docs/checkpoints/bakery_shop_b"
REFS = ROOT / "assets-source/refs/bakery"
W, H = 640, 400
CELLS = [
    (REFS / "ref-1.jpg", "reference ref-1 (approved, A17): twin front gables"),
    (REFS / "ref-2.jpg", "reference ref-2 (approved, A17)"),
    (DOCS / "eevee-three-quarter.jpg", "Blender EEVEE, textured + masks (evidence only)"),
    (DOCS / "eevee-front.jpg", "Blender EEVEE, shop front (bread = colour bands + masks)"),
    (DOCS / "eevee-gable-back.jpg", "Blender EEVEE, back and hip end"),
    (DOCS / "eevee-night.jpg", "Blender EEVEE, night: masks B x 4 on the glass"),
    (DOCS / "bakery_b-kit-play.jpg", "kit, play distance ~45 m: bakery_shop_b (left, front lit) and bakery_shop (right)"),
    (DOCS / "bakery_b-kit-top.jpg", "kit, from above: bakery_shop (top) and bakery_shop_b (bottom)"),
    (DOCS / "bakery_b-kit-scale.jpg", "kit, scale: both bakeries, house_cluster down the road"),
    (DOCS / "bakery_b-kit-close-sun.jpg", "kit, shop front ~6 m (sun side)"),
    (DOCS / "bakery_b-kit-close-shade.jpg", "kit, back ~7 m (shade side)"),
    (DOCS / "bakery_b-play.jpg", "play, variant forced: (0, 1) draws bakery_shop_b, (1, 1) bakery_shop"),
    (DOCS / "bakery_b-play-close-sun.jpg", "play, bakery_shop_b front ~7 m (sun side)"),
    (DOCS / "bakery_b-play-close-shade.jpg", "play, bakery_shop_b back ~9 m (shade side)"),
    (DOCS / "bakery_b-play-pair.jpg", "play, the pair on one road (variant left)"),
    (DOCS / "bakery_b-walk-eye.jpg", "play, walk mode eye level 1.7 m on the forecourt mouth"),
]


def cell(path: pathlib.Path, label: str) -> Image.Image:
    im = Image.open(path).convert("RGB")
    im.thumbnail((W, H))
    c = Image.new("RGB", (W, H + 26), (24, 24, 24))
    c.paste(im, ((W - im.width) // 2, 26 + (H - im.height) // 2))
    ImageDraw.Draw(c).text((8, 6), label, fill=(235, 235, 235))
    return c


comp = json.loads((ROOT / "assets-source/kit/bakery_shop_b/compress-report.json").read_text())
b = comp["budgets"]
exp = json.loads((ROOT / "assets-source/kit/bakery_shop_b/export-report.json").read_text())
tris = "/".join(str(exp["lodTris"][f"bakery_shop_b_lod{i}"]) for i in range(3))
title = (f"bakery_shop_b (bakery slot variant, PRD 6 Variety): review sheet. LOD {tris} tris, GLB "
         f"{b['glbBytes'] / 1000:.0f} KB, masks 1024 KTX2 {b['ktxBytes']['bc7'] / 1e6:.2f} MB BC7 / "
         f"{b['ktxBytes']['astc'] / 1e6:.2f} MB ASTC. Pending owner review.")
cells = [cell(p, label) for p, label in CELLS]
cols = 3
rows = (len(cells) + cols - 1) // cols
cw, ch = cells[0].size
sheet = Image.new("RGB", (cw * cols, ch * rows + 30), (24, 24, 24))
ImageDraw.Draw(sheet).text((8, 8), title, fill=(235, 235, 235))
for k, c in enumerate(cells):
    sheet.paste(c, ((k % cols) * cw, 30 + (k // cols) * ch))
sheet.save(DOCS / "sheet.jpg", quality=85)
print("wrote", DOCS / "sheet.jpg")
