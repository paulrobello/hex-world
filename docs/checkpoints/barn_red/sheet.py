"""barn_red owner review sheet (SKILL.md "Owner checkpoints"): the approved references, the textured
headless EEVEE stills (evidence only), and the in-engine shots with tilt-shift off (play distance,
sun-side and shade-side close-ups, eye level 1.7 m, the scale shot beside the oak, and the EEVEE
scale still beside both houses), labelled, 3 columns. A prop has no border seam.
Usage: python3 docs/checkpoints/barn_red/sheet.py (from the repo root; needs Pillow)."""
import pathlib

from PIL import Image, ImageDraw

ROOT = pathlib.Path(__file__).resolve().parents[3]
DOCS = ROOT / "docs/checkpoints/barn_red"
REFS = ROOT / "assets-source/refs/farm"
W, H = 640, 400
CELLS = [
    (REFS / "ref-1.jpg", "reference ref-1 (approved; A17: pitched roof)"),
    (REFS / "ref-2.jpg", "reference ref-2 (approved; gambrel overridden by A17)"),
    (DOCS / "still-hero.jpg", "Blender EEVEE, textured + AO (evidence only)"),
    (DOCS / "barn-play.jpg", "in engine, play distance ~60 m, tilt off"),
    (DOCS / "barn-close-sun.jpg", "in engine, sun side (doors), ~8 m"),
    (DOCS / "barn-close-shade.jpg", "in engine, shade side (back gable), ~9 m"),
    (DOCS / "barn-eye.jpg", "in engine, eye level 1.7 m, 12 m out"),
    (DOCS / "barn-scale.jpg", "in engine, scale: barn and the hero oak"),
    (DOCS / "still-scale.jpg", "EEVEE scale: twostorey | barn | cottage"),
]


def cell(path: pathlib.Path, label: str) -> Image.Image:
    im = Image.open(path).convert("RGB")
    im.thumbnail((W, H))
    c = Image.new("RGB", (W, H + 26), (24, 24, 24))
    c.paste(im, ((W - im.width) // 2, 26 + (H - im.height) // 2))
    ImageDraw.Draw(c).text((8, 6), label, fill=(235, 235, 235))
    return c


cells = [cell(p, label) for p, label in CELLS]
cols = 3
rows = (len(cells) + cols - 1) // cols
cw, ch = cells[0].size
sheet = Image.new("RGB", (cw * cols, ch * rows + 30), (24, 24, 24))
ImageDraw.Draw(sheet).text((8, 8), "barn_red (P3 farm): review sheet. LOD 1054/280/58 tris, GLB 90.8 KB, masks 1024 KTX2 ~0.25 MB per family", fill=(235, 235, 235))
for k, c in enumerate(cells):
    sheet.paste(c, ((k % cols) * cw, 30 + (k // cols) * ch))
sheet.save(DOCS / "sheet.jpg", quality=85)
print("wrote", DOCS / "sheet.jpg")
