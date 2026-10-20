"""Review sheet for the forest-floor props, fern_clump and log_fallen (P3, refs approved 2026-10-13):
the approved references, textured headless EEVEE stills, and the in-engine shots (`--tilt off`) from
forest_floor.json. Run from the repo root after run.sh: python3 docs/checkpoints/forest_floor/sheet.py"""
import pathlib
import sys

ROOT = pathlib.Path(__file__).resolve().parents[3]
sys.path.insert(0, str(ROOT / "scripts/assets"))
from PIL import Image, ImageDraw  # noqa: E402

DOCS = ROOT / "docs/checkpoints/forest_floor"
REFS = ROOT / "assets-source/refs/forest"
CELLS = [
    (REFS / "ref-1.jpg", "ref-1 (approved): ferns and the fallen log"),
    (REFS / "ref-2.jpg", "ref-2 (approved): ferns tucked at the trunk"),
    (DOCS / "fern_clump-eevee-low_side.jpg", "fern_clump, textured EEVEE (Blender AgX; evidence only)"),
    (DOCS / "log_fallen-eevee-three_quarter.jpg", "log_fallen, textured EEVEE (evidence only)"),
    (DOCS / "forest_floor-eevee-together.jpg", "EEVEE: log, fern and a 1.7 m figure"),
    (DOCS / "floor-play.jpg", "in engine, play distance, tilt off: the forest at (-2, 1)"),
    (DOCS / "floor-log-sun.jpg", "log close-up, sun side (about 6 m)"),
    (DOCS / "floor-log-shade.jpg", "log close-up, shade side (about 5 m)"),
    (DOCS / "floor-fern-sun.jpg", "fern in open sun, sun side (about 4 m)"),
    (DOCS / "floor-fern-shade.jpg", "fern, shade side, looking toward the sun (about 4 m)"),
    (DOCS / "floor-fern-trunk.jpg", "fern tucked at a spruce trunk, in the crown's shade (ref-2)"),
    (DOCS / "floor-eye.jpg", "eye level 1.7 m in the forest"),
    (DOCS / "floor-scale.jpg", "scale: the log, a spruce and a parked sedan"),
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
