"""wheat_field owner review sheet (SKILL.md "Owner checkpoints"): the approved reference, the textured
headless EEVEE stills (evidence only), and the in-engine shots with tilt-shift off (play distance, the
seam with the grass neighbour and with a second field, sun-side and shade-side close-ups, the scale
shot across the wheat to the farm's barn, eye level 1.7 m in walk mode, and inside the field after a
walk into it), labelled, 3 columns.
Usage: python3 docs/checkpoints/wheat_field/sheet.py (from the repo root; needs Pillow)."""
import pathlib

from PIL import Image, ImageDraw

ROOT = pathlib.Path(__file__).resolve().parents[3]
DOCS = ROOT / "docs/checkpoints/wheat_field"
REFS = ROOT / "assets-source/refs/farm"
W, H = 640, 400
CELLS = [
    (REFS / "ref-1.jpg", "reference ref-1 (approved, A17: wheat crop)"),
    (DOCS / "still-play.jpg", "Blender EEVEE, textured (evidence only)"),
    (DOCS / "still-close.jpg", "Blender EEVEE, close (evidence only)"),
    (DOCS / "wheat-play.jpg", "in engine, play distance ~100 m, tilt off"),
    (DOCS / "wheat-seam.jpg", "in engine, seams: grass margin, next field"),
    (DOCS / "wheat-close-sun.jpg", "in engine, sun side, ~6 m"),
    (DOCS / "wheat-close-sun-1s.jpg", "in engine, sun side 1 s later (wind sway)"),
    (DOCS / "wheat-close-shade.jpg", "in engine, shade side, ~6 m"),
    (DOCS / "wheat-scale.jpg", "in engine, scale: wheat to the farm's barn"),
    (DOCS / "wheat-walk-eye.jpg", "in engine, eye level 1.7 m (walk mode)"),
    (DOCS / "wheat-walk-inside.jpg", "in engine, walked into the field (not blocked)"),
]


def cell(path: pathlib.Path, label: str) -> Image.Image:
    im = Image.open(path).convert("RGB")
    im.thumbnail((W, H))
    c = Image.new("RGB", (W, H + 26), (24, 24, 24))
    c.paste(im, ((W - im.width) // 2, (H - im.height) // 2))
    ImageDraw.Draw(c).text((8, H + 6), label, fill=(230, 230, 230))
    return c


cols = 3
rows = (len(CELLS) + cols - 1) // cols
sheet = Image.new("RGB", (cols * W, rows * (H + 26)), (16, 16, 16))
for i, (p, label) in enumerate(CELLS):
    sheet.paste(cell(p, label), ((i % cols) * W, (i // cols) * (H + 26)))
sheet.save(DOCS / "sheet.jpg", quality=85)
print("wrote", DOCS / "sheet.jpg")
