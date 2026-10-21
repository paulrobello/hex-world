"""well_stone / well_stone_b owner review sheet (SKILL.md "Owner checkpoints"): the approved references, the
textured headless EEVEE stills of both wells (evidence only) and the scale still beside house_cottage, and in
engine with tilt-shift off: play distance, the sun-side and shade-side close-ups and a scale shot beside
cottage 0 for each well (kit scenario), then in play the overview and walk mode at 1.7 m (a lawn view and a
look into each well from its apron). A prop has no border seam.
Usage: python3 docs/checkpoints/well_stone/sheet.py (from the repo root; needs Pillow)."""
import json
import pathlib

from PIL import Image, ImageDraw

ROOT = pathlib.Path(__file__).resolve().parents[3]
DOCS = ROOT / "docs/checkpoints/well_stone"
REFS = ROOT / "assets-source/refs/well_stone"
W, H = 640, 400
CELLS = [
    (REFS / "ref-1.jpg", "reference ref-1: the covered well (approved 2026-10-20)"),
    (REFS / "ref-2.jpg", "reference ref-2: the open well (variant)"),
    (REFS / "ref-3.jpg", "reference ref-3: on a cottage lawn, on a flagstone apron"),
    (DOCS / "well_stone-eevee-three-quarter.jpg", "well_stone, Blender EEVEE textured + masks (evidence only)"),
    (DOCS / "well_stone-eevee-side.jpg", "well_stone, EEVEE: gable end, crank post, braces"),
    (DOCS / "well_stone-eevee-front.jpg", "well_stone, EEVEE: front, bucket on the capstones"),
    (DOCS / "well_stone_b-eevee-three-quarter.jpg", "well_stone_b, EEVEE: crossbeam, bucket over the opening"),
    (DOCS / "well_stone_b-eevee-top.jpg", "well_stone_b, EEVEE top: capstones, shaft, apron"),
    (DOCS / "well-eevee-scale.jpg", "EEVEE scale: well_stone_b, well_stone, house_cottage"),
    (DOCS / "well-play.jpg", "in engine, play distance ~37 m, tilt off: well_stone, cluster (-1, 0)"),
    (DOCS / "well-close-sun.jpg", "in engine, well_stone 5 m, sun side (front, bucket)"),
    (DOCS / "well-close-shade.jpg", "in engine, well_stone 5 m, shade side (back, -x)"),
    (DOCS / "well-scale.jpg", "in engine, well_stone beside cottage 0 (its east gable)"),
    (DOCS / "wellb-play.jpg", "in engine, play distance ~37 m: well_stone_b, cluster (-1, -1)"),
    (DOCS / "wellb-close-sun.jpg", "in engine, well_stone_b 5 m, sun side"),
    (DOCS / "wellb-close-shade.jpg", "in engine, well_stone_b 5 m, shade side"),
    (DOCS / "wellb-scale.jpg", "in engine, well_stone_b beside the longhouse (cottage 0 slot)"),
    (DOCS / "well-play-overview.jpg", "play: well_stone at (1,0) (right), well_stone_b at (0,-1) (top) and (-1,0) (left, rot 1)"),
    (DOCS / "well-walk-eye.jpg", "play walk mode, eye 1.7 m, well_stone from the lawn (4.4 m)"),
    (DOCS / "well-walk-apron.jpg", "play walk mode, standing on well_stone's apron, looking in"),
    (DOCS / "wellb-walk-eye.jpg", "play walk mode, eye 1.7 m, well_stone_b from the lawn (cluster (0,-1))"),
    (DOCS / "wellb-walk-apron.jpg", "play walk mode, standing on well_stone_b's apron, looking in"),
]


def cell(src, label: str) -> Image.Image:
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


def stats(p: str) -> str:
    comp = json.loads((ROOT / f"assets-source/kit/{p}/compress-report.json").read_text())
    b = comp["budgets"]
    exp = json.loads((ROOT / f"assets-source/kit/{p}/export-report.json").read_text())
    tris = "/".join(str(exp["lodTris"][f"{p}_lod{i}"]) for i in range(3))
    return (f"{p}: LOD {tris} tris, GLB {b['glbBytes'] / 1000:.0f} KB, masks 1024 KTX2 "
            f"{b['ktxBytes']['bc7'] / 1e6:.2f} MB BC7 / {b['ktxBytes']['astc'] / 1e6:.2f} MB ASTC")


title = f"well_stone review sheet. {stats('well_stone')}.  {stats('well_stone_b')}."
note = ("Owner WQ1-WQ3 unanswered, built to the recommendations: house_cluster hero 3 at (5.3, -7.7) yaw 60 east of "
        "cottage 0; the open well is a _b variant (about 1 cluster in 3); 1.58 m across the capstones, 0.9 m wall, "
        "2.4 m ridge, apron 2.77 m. Collider = LOD2 hull (apron rim, 1.385 m: grass and scatter keep off the apron); "
        "the walker stops at the wall (open) or the eaves (covered) and stands on the apron.")
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
