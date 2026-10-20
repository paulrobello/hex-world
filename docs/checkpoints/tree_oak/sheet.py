"""Review sheet for the scatter oaks, tree_oak and tree_oak_b (P3, refs approved 2026-10-13): the
approved references, textured EEVEE stills, the in-engine shots (`--tilt off`) from tree_oak.json, and
a wind difference image (|frame a - frame b| x 4) proving the crown moves. Run from the repo root after
run.sh: python3 docs/checkpoints/tree_oak/sheet.py <shots dir with oak-wind-a.png and oak-wind-b.png>"""
import pathlib
import sys

ROOT = pathlib.Path(__file__).resolve().parents[3]
sys.path.insert(0, str(ROOT / "scripts/assets"))
import numpy as np  # noqa: E402
from PIL import Image, ImageDraw  # noqa: E402

DOCS = ROOT / "docs/checkpoints/tree_oak"
REFS = ROOT / "assets-source/refs/forest"
shots = pathlib.Path(sys.argv[1]) if len(sys.argv) > 1 else ROOT / "target/shots"
a = np.asarray(Image.open(shots / "oak-wind-a.png").convert("RGB"), np.float32)
b = np.asarray(Image.open(shots / "oak-wind-b.png").convert("RGB"), np.float32)
diff = np.abs(a - b)
changed = float((diff.max(-1) > 8).mean())
Image.fromarray(np.clip(diff * 4, 0, 255).astype(np.uint8)).save(DOCS / "oak-wind-diff.jpg", quality=85)
print(f"wind pair: {changed:.4f} of pixels changed by more than 8 levels")
CELLS = [
    (REFS / "ref-1.jpg", "ref-1 (approved): broad oaks among the spruces"),
    (DOCS / "tree_oak-eevee-together.jpg", "EEVEE: hero oak (untextured, size only), tree_oak, 1.7 m figure, tree_oak_b"),
    (DOCS / "tree_oak-eevee-three_quarter.jpg", "tree_oak, textured EEVEE (Blender AgX; evidence only)"),
    (DOCS / "tree_oak_b-eevee-three_quarter.jpg", "tree_oak_b, textured EEVEE (evidence only)"),
    (DOCS / "oak-play.jpg", "in engine, play distance, tilt off: tree_oak (left), tree_oak_b, a spruce behind"),
    (DOCS / "oak-scale-hero.jpg", "scale: the oaks' forest (left), the forest-floor spruces, the hero oak (right)"),
    (DOCS / "oak-sun.jpg", "tree_oak close-up, sun side (about 10 m)"),
    (DOCS / "oak-shade.jpg", "tree_oak close-up, shade side, toward the sun (about 9 m)"),
    (DOCS / "oak-b-sun.jpg", "tree_oak_b close-up, sun side (about 8 m)"),
    (DOCS / "oak-b-shade.jpg", "tree_oak_b close-up from the south-east, cross light (about 9 m), tree_oak behind"),
    (DOCS / "oak-eye.jpg", "eye level 1.7 m under tree_oak's canopy"),
    (DOCS / "oak-wind-a.jpg", "wind: frame a"),
    (DOCS / "oak-wind-diff.jpg", f"wind: |a - b| x 4, 40 frames later ({changed:.1%} of pixels moved)"),
]
W, H, PAD, LABEL = 800, 500, 12, 28
cols, rows = 2, (len(CELLS) + 1) // 2
sheet = Image.new("RGB", (cols * (W + PAD) + PAD, rows * (H + LABEL + PAD) + PAD), (30, 30, 30))
draw = ImageDraw.Draw(sheet)
for i, (path, label) in enumerate(CELLS):
    img = Image.open(path).convert("RGB")
    img.thumbnail((W, H))
    x = PAD + (i % cols) * (W + PAD)
    y = PAD + (i // cols) * (H + LABEL + PAD)
    sheet.paste(img, (x + (W - img.width) // 2, y + LABEL + (H - img.height) // 2))
    draw.text((x + 4, y + 6), label, fill=(230, 230, 230))
sheet.save(DOCS / "sheet.jpg", quality=85)
print("wrote", DOCS / "sheet.jpg")
