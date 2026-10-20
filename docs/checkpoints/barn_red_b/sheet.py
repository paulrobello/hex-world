"""barn_red_b owner review sheet (SKILL.md "Owner checkpoints"): the approved references, the textured
headless EEVEE stills (evidence only), and the in-engine shots with tilt-shift off: play distance in
the kit scenario and in play (the variant drawn by its cell), the seam at the barn's back, close-ups
on the sun side, into the open shed and on the shade side, eye level 1.7 m in walk mode, and the
scale shot beside barn_red. Labelled, 3 columns.
Usage: python3 docs/checkpoints/barn_red_b/sheet.py (from the repo root; needs Pillow)."""
import json
import pathlib

from PIL import Image, ImageDraw

ROOT = pathlib.Path(__file__).resolve().parents[3]
DOCS = ROOT / "docs/checkpoints/barn_red_b"
REFS = ROOT / "assets-source/refs/farm"
W, H = 640, 400
CELLS = [
    (REFS / "ref-1.jpg", "reference ref-1 (approved; its lean-to shed)"),
    (REFS / "ref-2.jpg", "reference ref-2 (approved; gambrel overridden by A17)"),
    (DOCS / "still-hero.jpg", "Blender EEVEE, textured + AO (evidence only)"),
    (DOCS / "barn_b-play-south.jpg", "in engine (play), from the start camera's side: the open sheds"),
    (DOCS / "still-scale.jpg", "EEVEE scale: barn_red_b | barn_red (behind)"),
    (DOCS / "still-back.jpg", "EEVEE back: closed side, back gable"),
    (DOCS / "barn_b-play.jpg", "in engine (kit), play distance ~60 m, tilt off"),
    (DOCS / "barn_b-play-overview.jpg", "in engine (play): farms at (0,1) b, (1,0) red, (-1,0) b"),
    (DOCS / "barn_b-seam.jpg", "in engine (play): back 0.5 m from the (0,1)/(1,0) seam"),
    (DOCS / "barn_b-close-sun.jpg", "in engine, sun side (doors, hayloft), ~8 m"),
    (DOCS / "barn_b-close-shed.jpg", "in engine, into the open shed, ~7 m"),
    (DOCS / "barn_b-close-shade.jpg", "in engine, shade side (back gable), 7.5 m"),
    (DOCS / "barn_b-walk-eye.jpg", "in engine, walk mode eye level 1.7 m, the shed"),
    (DOCS / "barn_b-walk-doors.jpg", "in engine, walk mode eye level 1.7 m, the doors"),
    (DOCS / "barn_b-scale.jpg", "in engine, scale: barn_red (0,5) | barn_red_b (1,5)"),
]


def cell(path: pathlib.Path, label: str) -> Image.Image:
    im = Image.open(path).convert("RGB")
    im.thumbnail((W, H))
    c = Image.new("RGB", (W, H + 26), (24, 24, 24))
    c.paste(im, ((W - im.width) // 2, 26 + (H - im.height) // 2))
    ImageDraw.Draw(c).text((8, 6), label, fill=(235, 235, 235))
    return c


prov = json.loads((ROOT / "assets-source/kit/barn_red_b/provenance.json").read_text())
tris = "/".join(str(lod["tris"]) for lod in prov["lods"])
b = prov["compression"]["budgets"]
title = (f"barn_red_b (barn_red variant, farm): review sheet. LOD {tris} tris, GLB {b['glbBytes'] / 1000:.1f} KB, "
         f"masks 1024 KTX2 bc7 {b['ktxBytes']['bc7'] / 1e6:.2f} MB / astc {b['ktxBytes']['astc'] / 1e6:.2f} MB")
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
