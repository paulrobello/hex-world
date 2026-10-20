"""Rail owner review sheet (SKILL.md "Owner checkpoints"): the approved references, the textured
headless EEVEE stills (evidence only), and the in-engine shots with tilt-shift off (play distance,
the straight/bend seam, sun-side and shade-side close-ups, eye level, and the scale shot beside the
barn), labelled, 3 columns. There is no rail tile yet, so the pieces sit on meadow bases.
Usage: python3 docs/checkpoints/rail/sheet.py (from the repo root; needs Pillow)."""
import pathlib

from PIL import Image, ImageDraw

ROOT = pathlib.Path(__file__).resolve().parents[3]
DOCS = ROOT / "docs/checkpoints/rail"
REFS = ROOT / "assets-source/refs/rail"
W, H = 640, 400
CELLS = [
    (REFS / "ref-1.jpg", "reference ref-1 (approved: straight)"),
    (REFS / "ref-2.jpg", "reference ref-2 (approved; A17: bend across adjacent edges)"),
    (DOCS / "still-straight.jpg", "Blender EEVEE straight, textured + masks AO"),
    (DOCS / "still-bend.jpg", "Blender EEVEE bend (8 m arc), textured"),
    (DOCS / "still-close.jpg", "Blender EEVEE close-up ~4 m, sun side"),
    (DOCS / "still-scale.jpg", "EEVEE scale: sedan and cottage beside the track"),
    (DOCS / "rail-play.jpg", "in engine, play distance ~65 m, tilt off"),
    (DOCS / "rail-seam.jpg", "in engine, straight meets bend (world edge 3)"),
    (DOCS / "rail-close-sun.jpg", "in engine, sun side (south), ~5.5 m"),
    (DOCS / "rail-close-shade.jpg", "in engine, shade side (north), ~5.5 m"),
    (DOCS / "rail-eye.jpg", "in engine, eye level 1.6 m beside the track"),
    (DOCS / "rail-scale.jpg", "in engine, scale: track, barn and farm animals"),
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
ImageDraw.Draw(sheet).text(
    (8, 8),
    "rail (P3): review sheet. gauge 1.6 m, single track, edge midpoint to edge midpoint. "
    "straight 704/156/38 tris, 47.6 KB; bend 1456/504/152 tris, 86 KB; masks 512 KTX2 5-12 KB",
    fill=(235, 235, 235))
for k, c in enumerate(cells):
    sheet.paste(c, ((k % cols) * cw, 30 + (k // cols) * ch))
sheet.save(DOCS / "sheet.jpg", quality=85)
print("wrote", DOCS / "sheet.jpg")
