"""house_twostorey_b owner review sheet (SKILL.md "Owner checkpoints"): the approved references, the textured
headless EEVEE stills (evidence only), the trio beside house_twostorey and house_longhouse, and the
in-engine shots with tilt-shift off: play distance in a cluster, the sun-side and shade-side close-ups of
the yawed copy (world yaw 60) beside the approved house_twostorey from the same relative cameras, eye level
1.7 m, the scale shot with a sedan parked at park-2, and the shadows-off yaw check. A prop has no border
seam.
Usage: python3 docs/checkpoints/house_twostorey_b/sheet.py (from the repo root; needs Pillow)."""
import json
import pathlib

from PIL import Image, ImageDraw

ROOT = pathlib.Path(__file__).resolve().parents[3]
P = "house_twostorey_b"
DOCS = ROOT / "docs/checkpoints" / P
REFS = ROOT / "assets-source/refs/house_cluster"
W, H = 640, 400
CELLS = [
    (REFS / "ref-1.jpg", "reference ref-1 (approved 2026-10-13, timber-framed style)"),
    (REFS / "ref-2.jpg", "reference ref-2 (approved 2026-10-13)"),
    (DOCS / "eevee-three-quarter.jpg", "Blender EEVEE, textured + masks (evidence only)"),
    (DOCS / "eevee-front.jpg", "Blender EEVEE, front: jetty, close studding, door under it"),
    (DOCS / "eevee-back.jpg", "Blender EEVEE, back: braced frame, hipped roof"),
    (DOCS / "eevee-trio.jpg", "Blender EEVEE: longhouse, twostorey_b, twostorey at true scale"),
    (DOCS / "eevee-top.jpg", "Blender EEVEE, top: hips and ridge stack"),
    (DOCS / "eevee-night.jpg", "Blender EEVEE, night: masks B x 4 on the glass"),
    (DOCS / "twostorey_b-play.jpg", "in engine, play distance ~35 m, tilt off (cluster on (1,-2))"),
    (DOCS / "twostorey_b-close-sun.jpg", "in engine, jettied front ~6 m, world yaw 60 (sun side)"),
    (DOCS / "twostorey-close-sun.jpg", "same relative camera on house_twostorey, yaw 0 (approved)"),
    (DOCS / "twostorey_b-close-shade.jpg", "in engine, back + left hip ~7 m, world yaw 60 (shade side)"),
    (DOCS / "twostorey-close-shade.jpg", "same relative camera on house_twostorey (approved; compare)"),
    (DOCS / "twostorey_b-eye.jpg", "in engine, eye level 1.7 m on the drive (flagstones under jetty)"),
    (DOCS / "twostorey_b-scale.jpg", "in engine, scale: sedan parked at park-2, cottages either side"),
    (DOCS / "yaw-twostorey_b-sun.jpg", "shadows off: twostorey_b front, world yaw 60"),
    (DOCS / "yaw-twostorey-sun.jpg", "shadows off: house_twostorey front, yaw 0"),
]
NOTE = ("Not yet drawn in play: one house-2 slot in three draws it once the HERO_VARIANTS row lands "
        "(integration.patch); the origin cell's slot does, so house_cluster thumbnails change then.")


def cell(path: pathlib.Path, label: str) -> Image.Image:
    im = Image.open(path).convert("RGB")
    im.thumbnail((W, H))
    c = Image.new("RGB", (W, H + 26), (24, 24, 24))
    c.paste(im, ((W - im.width) // 2, 26 + (H - im.height) // 2))
    ImageDraw.Draw(c).text((8, 6), label, fill=(235, 235, 235))
    return c


comp = json.loads((ROOT / f"assets-source/kit/{P}/compress-report.json").read_text())
b = comp["budgets"]
exp = json.loads((ROOT / f"assets-source/kit/{P}/export-report.json").read_text())
tris = "/".join(str(exp["lodTris"][f"{P}_lod{i}"]) for i in range(3))
title = (f"{P} (house_twostorey variant): review sheet. LOD {tris} tris, GLB {b['glbBytes'] / 1000:.0f} KB, "
         f"masks 1024 KTX2 {b['ktxBytes']['bc7'] / 1e6:.2f} MB BC7 / {b['ktxBytes']['astc'] / 1e6:.2f} MB ASTC; "
         f"collider x +-2.75, z +-3.75 m (twostorey x +-2.8, z -3.8..4.25)")
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
