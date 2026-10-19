"""bakery_shop owner review sheet (SKILL.md "Owner checkpoints"): the approved references, the textured
headless EEVEE stills by day and night (evidence only), and the in-engine shots with tilt-shift off
(play distance, sun-side and shade-side close-ups, eye level 1.7 m, the scale shot beside the
house_cluster cottages), labelled, 3 columns. A prop has no border seam.
Usage: python3 docs/checkpoints/bakery_shop/sheet.py (from the repo root; needs Pillow)."""
import json
import pathlib

from PIL import Image, ImageDraw

ROOT = pathlib.Path(__file__).resolve().parents[3]
DOCS = ROOT / "docs/checkpoints/bakery_shop"
REFS = ROOT / "assets-source/refs/bakery"
W, H = 640, 400
CELLS = [
    (REFS / "ref-1.jpg", "reference ref-1 (approved, A17)"),
    (REFS / "ref-2.jpg", "reference ref-2 (approved, A17)"),
    (DOCS / "eevee-three-quarter.jpg", "Blender EEVEE, textured + masks (evidence only)"),
    (DOCS / "eevee-front.jpg", "Blender EEVEE, shop front (bread = colour bands + masks)"),
    (DOCS / "eevee-night.jpg", "Blender EEVEE, night: masks B x 4 on the glass"),
    (DOCS / "bakery-play.jpg", "in engine, play distance ~45 m, tilt off (walls: see yaw note)"),
    (DOCS / "bakery-close-sun.jpg", "in engine, shop front ~6 m (dark: yaw shading defect)"),
    (DOCS / "bakery-close-shade.jpg", "in engine, back and left gable ~8 m (yaw shading defect)"),
    (DOCS / "bakery-eye.jpg", "in engine, eye level 1.7 m, 11 m out (yaw shading defect)"),
    (DOCS / "bakery-scale.jpg", "in engine, scale: bakery (front right), house_cluster down the road"),
    (DOCS / "yaw-bakery-plus-x.jpg", "shadows off: bakery +x gable (N.L 0.55) reads unlit, yaw 90"),
    (DOCS / "yaw-cluster-plus-x.jpg", "shadows off: twostorey yaw 0 lit; cottages yaw 90/270 dark"),
]
NOTE = ("In-engine wall shading is not representative: pieces placed with a hero yaw (bakery 90, cottages 90/270) "
        "shade their walls as if unrotated, while yaw-0 house_twostorey is lit (renderer finding, orchestrator).")


def cell(path: pathlib.Path, label: str) -> Image.Image:
    im = Image.open(path).convert("RGB")
    im.thumbnail((W, H))
    c = Image.new("RGB", (W, H + 26), (24, 24, 24))
    c.paste(im, ((W - im.width) // 2, 26 + (H - im.height) // 2))
    ImageDraw.Draw(c).text((8, 6), label, fill=(235, 235, 235))
    return c


prov = json.loads((ROOT / "assets-source/kit/bakery_shop/compress-report.json").read_text())
b = prov["budgets"]
exp = json.loads((ROOT / "assets-source/kit/bakery_shop/export-report.json").read_text())
tris = "/".join(str(exp["lodTris"][f"bakery_shop_lod{i}"]) for i in range(3))
title = (f"bakery_shop (P3 village): review sheet. LOD {tris} tris, GLB {b['glbBytes'] / 1000:.0f} KB, "
         f"masks 1024 KTX2 {b['ktxBytes']['bc7'] / 1e6:.2f} MB BC7 / {b['ktxBytes']['astc'] / 1e6:.2f} MB ASTC")
cells = [cell(p, label) for p, label in CELLS]
cols = 3
rows = (len(cells) + cols - 1) // cols
cw, ch = cells[0].size
sheet = Image.new("RGB", (cw * cols, ch * rows + 50), (24, 24, 24))
ImageDraw.Draw(sheet).text((8, 8), title, fill=(235, 235, 235))
ImageDraw.Draw(sheet).text((8, 28), NOTE, fill=(255, 193, 7))
for k, c in enumerate(cells):
    sheet.paste(c, ((k % cols) * cw, 50 + (k // cols) * ch))
sheet.save(DOCS / "sheet.jpg", quality=85)
print("wrote", DOCS / "sheet.jpg")
