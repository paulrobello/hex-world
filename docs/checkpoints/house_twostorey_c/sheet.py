"""house_twostorey_c owner review sheet (SKILL.md "Owner checkpoints"): the approved references (with ref-1's
middle house cropped), the textured headless EEVEE stills (evidence only), the lineup beside house_twostorey
and house_twostorey_b, and the in-engine shots with tilt-shift off: play distance, the sun-side and shade-side
close-ups, the same two cameras on the approved house_twostorey in the same slot and yaw, eye level 1.7 m, the
scale shot with sedans parked at park-0 and park-1, and in play (integration patch applied) four clusters
and walk-mode eye level. A prop has no border seam.
Usage: python3 docs/checkpoints/house_twostorey_c/sheet.py (from the repo root; needs Pillow)."""
import json
import pathlib

from PIL import Image, ImageDraw

ROOT = pathlib.Path(__file__).resolve().parents[3]
P = "house_twostorey_c"
DOCS = ROOT / "docs/checkpoints" / P
REFS = ROOT / "assets-source/refs/house_cluster"
W, H = 640, 400
CROP = (REFS / "ref-1.jpg", (840, 300, 1300, 760))  # ref-1's middle house: stone ground floor, cross gable
CELLS = [
    (REFS / "ref-1.jpg", "reference ref-1 (approved 2026-10-13)"),
    (CROP, "ref-1 crop: middle house, rubble ground floor and front cross gable"),
    (DOCS / "eevee-three-quarter.jpg", "Blender EEVEE, textured + masks (evidence only)"),
    (DOCS / "eevee-front.jpg", "Blender EEVEE, front: gabled wing, lean-to door hood"),
    (DOCS / "eevee-back.jpg", "Blender EEVEE, back and -y gable: quoins, string course"),
    (DOCS / "eevee-trio.jpg", "Blender EEVEE: twostorey_b, twostorey_c, twostorey (the slot's three designs)"),
    (DOCS / "eevee-top.jpg", "Blender EEVEE, top: T plan, cross ridge, end stack"),
    (DOCS / "eevee-night.jpg", "Blender EEVEE, night: masks B x 4 on the glass"),
    (DOCS / "twostorey_c-play.jpg", "in engine, play distance ~35 m, tilt off (cluster on (-1,-1))"),
    (DOCS / "twostorey_c-close-sun.jpg", "in engine, wing front ~6 m, yaw 0 (sun side)"),
    (DOCS / "twostorey_c-close-shade.jpg", "in engine, back + stack gable ~7 m (shade side)"),
    (DOCS / "twostorey-close-sun.jpg", "same sun camera on house_twostorey (approved; compare)"),
    (DOCS / "twostorey-close-shade.jpg", "same shade camera on house_twostorey (approved; compare)"),
    (DOCS / "twostorey_c-eye.jpg", "in engine, kit camera at 1.7 m in the yard"),
    (DOCS / "twostorey_c-scale.jpg", "in engine, scale from the drive: sedans at park-0/park-1, both cottages"),
    (DOCS / "twostorey_c-play-overview.jpg", "play (patch applied): _c at (1,0), _b at (0,-1) and (-1,1)"),
    (DOCS / "twostorey_c-play-trio.jpg", "play ~50 m, one scale: twostorey (left), _b (top), _c (right)"),
    (DOCS / "twostorey_c-walk-eye.jpg", "play walk mode, eye level 1.7 m, yard of the (1, 0) cluster"),
    (DOCS / "twostorey_c-walk-corner.jpg", "play walk mode at the paved corner beside the wing"),
]


def cell(src, label: str) -> Image.Image:
    if isinstance(src, tuple):
        im = Image.open(src[0]).convert("RGB").crop(src[1])
    else:
        im = Image.open(src).convert("RGB")
    if im.width > W or im.height > H:
        im.thumbnail((W, H))
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
title = (f"{P} (house_twostorey variant): review sheet. LOD {tris} tris, GLB {b['glbBytes'] / 1000:.0f} KB, "
         f"masks 1024 KTX2 {b['ktxBytes']['bc7'] / 1e6:.2f} MB BC7 / {b['ktxBytes']['astc'] / 1e6:.2f} MB ASTC")
def mean_luma(path: pathlib.Path) -> float:
    im = Image.open(path).convert("L")
    return sum(i * n for i, n in enumerate(im.histogram())) / (im.width * im.height)


shade = {n: mean_luma(DOCS / f"{n}-close-shade.jpg") for n in ("twostorey_c", "twostorey")}
note = (f"Shade side faces away from the kit sun: same camera and yaw, mean luma twostorey_c {shade['twostorey_c']:.0f}, "
        f"approved twostorey {shade['twostorey']:.0f}. Walls inside the parking two-storey box (x ±2.5, |y| <= 3.6); "
        "box collider (LOD2) inside house_twostorey's ground box, reach 4.63 m (4.75); door hood inside the pad "
        "doorstep box. Slot split over 1600 cells: 1064 authored / 287 _b / 249 _c.")
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
