"""windmill_b owner review sheet (SKILL.md "Owner checkpoints"): the approved references, the headless EEVEE
stills with the masks bound (evidence only), and the in-engine shots with tilt-shift off: the play scenario
with the variant drawn in the windmill slot (play distance and the same view 1 s later, sails +24°, sun and
shade close-ups, the scale shot beside windmill and a house_cluster, eye level 1.7 m in walk mode) and the
kit scenario (a closer pair 1 s apart, both mills on their meadows). A prop has no border seam: the play
shots show the windmill tile meeting its neighbours, unchanged. 3 columns.
Usage: python3 docs/checkpoints/windmill_b/sheet.py (from the repo root; needs Pillow)."""
import json
import pathlib

from PIL import Image, ImageDraw

ROOT = pathlib.Path(__file__).resolve().parents[3]
DOCS = ROOT / "docs/checkpoints/windmill_b"
REFS = ROOT / "assets-source/refs/windmill"
W, H = 640, 400
CELLS = [
    (REFS / "ref-1.jpg", "reference ref-1 (approved; owner A17: white, flat tile, fantail still)"),
    (REFS / "ref-2.jpg", "reference ref-2 (approved): sails, cap gallery, fantail"),
    (DOCS / "eevee-three-quarter.jpg", "Blender EEVEE + masks (evidence only): smock mill"),
    (DOCS / "eevee-front.jpg", "Blender EEVEE, front: double-shuttered patent sails"),
    (DOCS / "eevee-back.jpg", "Blender EEVEE, back: ogee cap, finial, still fantail"),
    (DOCS / "eevee-stage.jpg", "Blender EEVEE, stage ~8 m: rail, struts, both doors"),
    (DOCS / "windmill_b-play.jpg", "play ~50 m, variant forced: (0, 1) draws windmill_b"),
    (DOCS / "windmill_b-play-1s.jpg", "play, same view 1 s later (--clock fixed): sails +24°"),
    (DOCS / "windmill_b-play-scale.jpg", "play, scale: windmill_b (centre), windmill (1, 0), house_cluster"),
    (DOCS / "windmill_b-play-close-sun.jpg", "play, base and stage ~8 m (sun side)"),
    (DOCS / "windmill_b-play-close-shade.jpg", "play, shade side ~8 m: smock luma 22 (sun side 157), as house_twostorey north wall (22)"),
    (DOCS / "windmill_b-walk-eye.jpg", "play, walk mode eye level 1.7 m, 9 m from the tower"),
    (DOCS / "windmill_b-kit-t1.jpg", "kit, t1 (fixed clock)"),
    (DOCS / "windmill_b-kit-t2.jpg", "kit, t2 = t1 + 1 s (sails +24°)"),
    (DOCS / "windmill_b-kit-pair.jpg", "kit: windmill (left, 3,-1) and windmill_b (right, 4,-1)"),
    (DOCS / "eevee-night.jpg", "Blender EEVEE, night: masks B x 4 on the glass"),
]


def cell(path: pathlib.Path, label: str) -> Image.Image:
    im = Image.open(path).convert("RGB")
    im.thumbnail((W, H))
    c = Image.new("RGB", (W, H + 26), (24, 24, 24))
    c.paste(im, ((W - im.width) // 2, 26 + (H - im.height) // 2))
    ImageDraw.Draw(c).text((8, 6), label, fill=(235, 235, 235))
    return c


comp = json.loads((ROOT / "assets-source/kit/windmill_b/compress-report.json").read_text())
b = comp["budgets"]
exp = json.loads((ROOT / "assets-source/kit/windmill_b/export-report.json").read_text())
tris = "/".join(str(exp["lodTris"][f"windmill_b_lod{i}"]) for i in range(3))
title = (f"windmill_b (windmill slot variant, PRD 5.4 row 2b / 6 Variety): review sheet. LOD {tris} tris "
         f"(body + rotor), GLB {b['glbBytes'] / 1000:.0f} KB, masks 1024 KTX2 {b['ktxBytes']['bc7'] / 1e6:.2f} MB "
         f"BC7 / {b['ktxBytes']['astc'] / 1e6:.2f} MB ASTC. Pending owner review.")
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
