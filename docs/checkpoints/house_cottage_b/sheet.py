"""house_cottage_b owner review sheet (SKILL.md "Owner checkpoints"): the approved references (with ref-1's
slate-roofed house cropped), the textured headless EEVEE stills (evidence only), the lineup beside
house_cottage and house_longhouse, and the in-engine shots with tilt-shift off: play distance, the sun-side
and shade-side close-ups of the yawed copy (yaw 270), the shade-side camera on the approved house_cottage in
the same slot and yaw, eye level 1.7 m, and the scale shot with sedans parked at park-0 and park-1. A prop
has no border seam.
Usage: python3 docs/checkpoints/house_cottage_b/sheet.py (from the repo root; needs Pillow)."""
import json
import pathlib

from PIL import Image, ImageDraw

ROOT = pathlib.Path(__file__).resolve().parents[3]
P = "house_cottage_b"
DOCS = ROOT / "docs/checkpoints" / P
REFS = ROOT / "assets-source/refs/house_cluster"
W, H = 640, 400
CROP = (REFS / "ref-1.jpg", (1180, 380, 1600, 760))  # ref-1's slate-roofed right-hand house
CELLS = [
    (REFS / "ref-1.jpg", "reference ref-1 (approved 2026-10-13)"),
    (CROP, "ref-1 crop: the slate-roofed house this variant follows"),
    (DOCS / "eevee-three-quarter.jpg", "Blender EEVEE, textured + masks (evidence only)"),
    (DOCS / "eevee-front.jpg", "Blender EEVEE, front: gabled porch, twin ridge stacks"),
    (DOCS / "eevee-back.jpg", "Blender EEVEE, back and gable: quoins, bargeboards"),
    (DOCS / "eevee-trio.jpg", "Blender EEVEE: longhouse, cottage_b, cottage (the slot's three designs)"),
    (DOCS / "eevee-top.jpg", "Blender EEVEE, top: porch valley and stacks on the ridge"),
    (DOCS / "eevee-night.jpg", "Blender EEVEE, night: masks B x 4 on the glass"),
    (DOCS / "cottage_b-play.jpg", "in engine, play distance ~35 m, tilt off (cluster on (-1,-1))"),
    (DOCS / "cottage_b-close-sun.jpg", "in engine, back + gable ~9 m, yaw 270 (sun side)"),
    (DOCS / "cottage_b-close-shade.jpg", "in engine, yard front + porch ~6 m, yaw 270 (shade side)"),
    (DOCS / "cottage_b-cottage-shade.jpg", "same camera on house_cottage, yaw 270 (approved; compare)"),
    (DOCS / "cottage_b-eye.jpg", "in engine, kit camera at 1.7 m on the driveway"),
    (DOCS / "cottage_b-scale.jpg", "in engine, scale: sedans at park-0 and park-1, longhouse opposite"),
    (DOCS / "cottage_b-play-overview.jpg", "play (integration patch applied): 4 clusters, cottage_b in 3"),
    (DOCS / "cottage_b-walk-eye.jpg", "play walk mode, eye level 1.7 m, yard of the (1, 0) cluster"),
]


def cell(src, label: str) -> Image.Image:
    if isinstance(src, tuple):
        im = Image.open(src[0]).convert("RGB").crop(src[1])
    else:
        im = Image.open(src).convert("RGB")
    im.thumbnail((W, H)) if im.width > W or im.height > H else None
    if im.width < W and im.height < H:
        s = min(W / im.width, H / im.height)
        im = im.resize((int(im.width * s), int(im.height * s)), Image.LANCZOS)
    c = Image.new("RGB", (W, H + 26), (24, 24, 24))
    c.paste(im, ((W - im.width) // 2, 26 + (H - im.height) // 2))
    ImageDraw.Draw(c).text((8, 6), label, fill=(235, 235, 235))
    return c


comp = json.loads((ROOT / f"assets-source/kit/{P}/compress-report.json").read_text())
b = comp["budgets"]
exp = json.loads((ROOT / f"assets-source/kit/{P}/export-report.json").read_text())
tris = "/".join(str(exp["lodTris"][f"{P}_lod{i}"]) for i in range(3))
title = (f"{P} (house_cottage variant): review sheet. LOD {tris} tris, GLB {b['glbBytes'] / 1000:.0f} KB, "
         f"masks 1024 KTX2 {b['ktxBytes']['bc7'] / 1e6:.2f} MB BC7 / {b['ktxBytes']['astc'] / 1e6:.2f} MB ASTC")
def mean_luma(path: pathlib.Path, box: tuple) -> float:
    im = Image.open(path).convert("L").crop(box)
    return sum(i * n for i, n in enumerate(im.histogram())) / (im.width * im.height)


# Plaster-only panels (no timber, window, quoin or roof in the box) on the shade walls of the two
# same-camera, same-yaw shots, picked from a pixel grid over each image.
PANELS = {"cottage_b-close-shade.jpg": [(20, 610, 120, 700), (510, 500, 590, 580), (900, 430, 960, 480)],
          "cottage_b-cottage-shade.jpg": [(200, 600, 280, 700), (600, 420, 680, 480)]}
lum = {k: [mean_luma(DOCS / k, bx) for bx in v] for k, v in PANELS.items()}
shade, ref = lum["cottage_b-close-shade.jpg"], lum["cottage_b-cottage-shade.jpg"]
note = (f"Shade side faces away from the kit sun. Plaster panel luma, same camera and yaw: cottage_b "
        f"{min(shade):.0f}-{max(shade):.0f}, approved cottage {min(ref):.0f}-{max(ref):.0f}. Walls inside "
        "house_cottage's 4.4 x 6.4 m, collider inside its ground box, porch inside the parking doorstep box. "
        "Slate tint is house_twostorey's.")
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
print("wrote", DOCS / "sheet.jpg")
