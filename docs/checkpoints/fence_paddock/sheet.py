"""fence_paddock owner review sheet (SKILL.md "Owner checkpoints"): the approved farm references, the
textured headless EEVEE stills (evidence only), and the in-engine shots with tilt-shift off on the farm
replica in the kit scenario (play distance with the barn, bales and animals; overhead; sun-side and
shade-side close-ups; eye level inside the paddock; the scale shot beside the cow and the barn; far,
LOD1), labelled, 3 columns. A prop has no border seam.
Usage: python3 docs/checkpoints/fence_paddock/sheet.py (from the repo root; needs Pillow)."""
import pathlib

from PIL import Image, ImageDraw

ROOT = pathlib.Path(__file__).resolve().parents[3]
DOCS = ROOT / "docs/checkpoints/fence_paddock"
REFS = ROOT / "assets-source/refs/farm"
W, H = 640, 400
CELLS = [
    (REFS / "ref-1.jpg", "reference ref-1 (approved): the paddock fence"),
    (REFS / "ref-2.jpg", "reference ref-2 (approved): post-and-rail"),
    (DOCS / "still-hero.jpg", "Blender EEVEE, textured + masks.A (evidence only)"),
    (DOCS / "fence-play.jpg", "in engine, play distance ~60 m, tilt off"),
    (DOCS / "fence-top.jpg", "in engine, overhead: outline, gate to the yard"),
    (DOCS / "fence-close-sun.jpg", "in engine, sun side (south-east run), ~4 m"),
    (DOCS / "fence-close-shade.jpg", "in engine, shade side (yard face, gate), ~6 m"),
    (DOCS / "fence-eye.jpg", "in engine, eye level 1.7 m inside the paddock"),
    (DOCS / "fence-scale.jpg", "in engine, scale: fence, cow, bales and barn"),
    (DOCS / "still-close.jpg", "EEVEE close-up: split rails at a corner"),
    (DOCS / "fence-far.jpg", "in engine, ~170 m (LOD1)"),
]
NOTE = ("Shade side measures 13-15/255, the same as the round bale's shade side in that frame (14) and the "
        "barn's sun-averted wall (11-21): sky-only light on north faces, not the asset.")


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
sheet = Image.new("RGB", (cw * cols, ch * rows + 52), (24, 24, 24))
d = ImageDraw.Draw(sheet)
d.text((8, 8), "fence_paddock (P3 farm): review sheet. LOD 1172/568/78 tris, GLB 168.6 KB, one plaster-tinted "
               "timber slot, analytic 64 px masks (KTX2 ~0.8 KB per family), 27 posts, 3 m gate", fill=(235, 235, 235))
d.text((8, 28), NOTE, fill=(200, 200, 200))
for k, c in enumerate(cells):
    sheet.paste(c, ((k % cols) * cw, 52 + (k // cols) * ch))
sheet.save(DOCS / "sheet.jpg", quality=85)
print("wrote", DOCS / "sheet.jpg")
