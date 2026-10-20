"""Review sheet for tree_oak_c, the third scatter-oak variant (forest refs approved 2026-10-13): the
approved reference, textured EEVEE stills (the oak alone, under its crown, a level lineup with the hero
oak, tree_oak and tree_oak_b, and an orthographic elevation of the three scatter oaks), the in-engine
shots (`--tilt off`) from oakc-kit.json and oakc-play.json, and a wind difference image
(|frame a - frame b| x 4, 60 frames = 1 s apart) proving the crown moves. Run from the repo root after
run.sh: python3 docs/checkpoints/tree_oak_c/sheet.py <shots dir with oakc-kit-wind-a.png and -b.png>"""
import pathlib
import sys

ROOT = pathlib.Path(__file__).resolve().parents[3]
sys.path.insert(0, str(ROOT / "scripts/assets"))
import numpy as np  # noqa: E402
from PIL import Image, ImageDraw  # noqa: E402

DOCS = ROOT / "docs/checkpoints/tree_oak_c"
REFS = ROOT / "assets-source/refs/forest"
shots = pathlib.Path(sys.argv[1]) if len(sys.argv) > 1 else ROOT / "target/shots"
a = np.asarray(Image.open(shots / "oakc-kit-wind-a.png").convert("RGB"), np.float32)
b = np.asarray(Image.open(shots / "oakc-kit-wind-b.png").convert("RGB"), np.float32)
diff = np.abs(a - b)
changed = float((diff.max(-1) > 8).mean())
Image.fromarray(np.clip(diff * 4, 0, 255).astype(np.uint8)).save(DOCS / "oakc-kit-wind-diff.jpg", quality=85)
print(f"wind pair: {changed:.4f} of pixels changed by more than 8 levels")
CELLS = [
    (REFS / "ref-1.jpg", "ref-1 (approved): broad oaks among the spruces"),
    (DOCS / "tree_oak_c-eevee-three_quarter.jpg", "tree_oak_c, textured EEVEE (Blender AgX; evidence only): 7.0 m tall, 9.6 m crown"),
    (DOCS / "oaks-elevation.jpg", "EEVEE side elevation, one scale: tree_oak (9.2 m), 1.7 m figure, tree_oak_b (7.9 m), tree_oak_c (7.0 m)"),
    (DOCS / "oaks-lineup.jpg", "EEVEE lineup: hero oak (13.3 m), tree_oak, tree_oak_b, 1.7 m figure, tree_oak_c"),
    (DOCS / "oakc-kit-play.jpg", "kit, play distance: tree_oak_c (front), tree_oak (left), tree_oak_b (right), a spruce behind"),
    (DOCS / "oakc-kit-scale.jpg", "kit, scale: the scatter-oak grove (left), the forest-floor spruces, the hero oak (right)"),
    (DOCS / "oakc-kit-sun.jpg", "tree_oak_c close-up, sun side (about 10 m)"),
    (DOCS / "oakc-kit-shade.jpg", "tree_oak_c close-up, shade side from the west, toward the sun (about 9 m)"),
    (DOCS / "oakc-kit-shade-tree_oak.jpg", "tree_oak from the same shade angle: the same leaf sheen toward the sun (shared canopy map)"),
    (DOCS / "oakc-kit-eye.jpg", "1.7 m camera 6 m from tree_oak_c's trunk: the low split and the level limbs"),
    (DOCS / "tree_oak_c-eevee-under.jpg", "EEVEE, 1.7 m under the crown (evidence only)"),
    (DOCS / "oakc-play.jpg", "play, seed 1: oak (1, 0), tree_oak_c behind the hero and tree_oak left; oak (2, -1), two tree_oak_c left of the hero"),
    (DOCS / "oakc-play-close.jpg", "play: tree_oak_c on (1, 0) from the east (about 13 m), the hero oak behind on the left"),
    (DOCS / "oakc-play-eye.jpg", "play, walk mode eye level 1.7 m, facing west at tree_oak_c (walk mode draws a black sky on main)"),
    (DOCS / "oakc-kit-wind-a.jpg", "wind: frame a"),
    (DOCS / "oakc-kit-wind-diff.jpg", f"wind: |a - b| x 4, 60 frames (1 s) later ({changed:.1%} of pixels moved)"),
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
