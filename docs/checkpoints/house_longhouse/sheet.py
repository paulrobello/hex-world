"""house_longhouse owner review sheet (SKILL.md "Owner checkpoints"): the approved references, the textured
headless EEVEE stills (evidence only), the trio beside house_cottage and house_twostorey, and the in-engine
shots with tilt-shift off: play distance, the sun-side close-up of the yawed copy (yaw 90), the shade-side
close-up beside the approved house_cottage in the same pose, eye level 1.7 m, and the scale shot with a
sedan parked at park-0. A prop has no border seam.
Usage: python3 docs/checkpoints/house_longhouse/sheet.py (from the repo root; needs Pillow)."""
import json
import pathlib

from PIL import Image, ImageDraw

ROOT = pathlib.Path(__file__).resolve().parents[3]
P = "house_longhouse"
DOCS = ROOT / "docs/checkpoints" / P
REFS = ROOT / "assets-source/refs/house_cluster"
W, H = 640, 400
CELLS = [
    (REFS / "ref-1.jpg", "reference ref-1 (approved 2026-10-13)"),
    (REFS / "ref-2.jpg", "reference ref-2 (approved 2026-10-13)"),
    (DOCS / "eevee-three-quarter.jpg", "Blender EEVEE, textured + masks (evidence only)"),
    (DOCS / "eevee-front.jpg", "Blender EEVEE, front: hall + close-studded cross wing"),
    (DOCS / "eevee-back.jpg", "Blender EEVEE, back: the L and the roof valley"),
    (DOCS / "eevee-trio.jpg", "Blender EEVEE: twostorey, longhouse, cottage at true scale"),
    (DOCS / "eevee-night.jpg", "Blender EEVEE, night: masks B x 4 on the glass"),
    (DOCS / "longhouse-play.jpg", "in engine, play distance ~35 m, tilt off (cluster on (-1,-1))"),
    (DOCS / "longhouse-close-sun.jpg", "in engine, yard front ~6 m, yaw 90 (sun side)"),
    (DOCS / "longhouse-close-shade.jpg", "in engine, back + hall gable ~8 m, yaw 90 (shade side)"),
    (DOCS / "longhouse-cottage-shade.jpg", "same camera on house_cottage, yaw 90 (approved; compare)"),
    (DOCS / "longhouse-eye.jpg", "in engine, eye level 1.7 m on the driveway"),
    (DOCS / "longhouse-scale.jpg", "in engine, scale: sedan parked at park-0, twostorey beyond"),
]


def cell(path: pathlib.Path, label: str) -> Image.Image:
    im = Image.open(path).convert("RGB")
    im.thumbnail((W, H))
    c = Image.new("RGB", (W, H + 26), (24, 24, 24))
    c.paste(im, ((W - im.width) // 2, 26 + (H - im.height) // 2))
    ImageDraw.Draw(c).text((8, 6), label, fill=(235, 235, 235))
    return c


def mean_luma(path: pathlib.Path, box: tuple) -> float:
    im = Image.open(path).convert("L").crop(box)
    return sum(i * n for i, n in enumerate(im.histogram())) / (im.width * im.height)


comp = json.loads((ROOT / f"assets-source/kit/{P}/compress-report.json").read_text())
b = comp["budgets"]
exp = json.loads((ROOT / f"assets-source/kit/{P}/export-report.json").read_text())
tris = "/".join(str(exp["lodTris"][f"{P}_lod{i}"]) for i in range(3))
title = (f"{P} (house_cluster variant): review sheet. LOD {tris} tris, GLB {b['glbBytes'] / 1000:.0f} KB, "
         f"masks 1024 KTX2 {b['ktxBytes']['bc7'] / 1e6:.2f} MB BC7 / {b['ktxBytes']['astc'] / 1e6:.2f} MB ASTC")
# Plaster-only panels (no timber, window or roof in the box) on the shade walls of the two same-camera,
# same-yaw shots, picked from a pixel grid over each image.
PANELS = {"longhouse-close-shade.jpg": [(560, 650, 640, 720), (1240, 700, 1300, 780)],
          "longhouse-cottage-shade.jpg": [(1000, 600, 1100, 700), (1020, 760, 1100, 820), (820, 620, 900, 700)]}
lum = {k: [mean_luma(DOCS / k, bx) for bx in v] for k, v in PANELS.items()}
shade, ref = lum["longhouse-close-shade.jpg"], lum["longhouse-cottage-shade.jpg"]
note = (f"Shade side faces away from the kit sun (200, 400, 150). Plaster panel luma, same camera and yaw: longhouse "
        f"{min(shade):.0f}-{max(shade):.0f}, approved cottage {min(ref):.0f}-{max(ref):.0f}. "
        f"Walls stay inside house_cottage's 4.4 x 6.4 m.")
cells = [cell(p, label) for p, label in CELLS]
cols = 3
rows = (len(cells) + cols - 1) // cols
cw, ch = cells[0].size
sheet = Image.new("RGB", (cw * cols, ch * rows + 50), (24, 24, 24))
ImageDraw.Draw(sheet).text((8, 8), title, fill=(235, 235, 235))
ImageDraw.Draw(sheet).text((8, 28), note, fill=(255, 193, 7))
for k, c in enumerate(cells):
    sheet.paste(c, ((k % cols) * cw, 50 + (k // cols) * ch))
sheet.save(DOCS / "sheet.jpg", quality=85)
print("wrote", DOCS / "sheet.jpg", note)
