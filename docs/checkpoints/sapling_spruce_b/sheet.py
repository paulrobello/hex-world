"""Review sheet for sapling_spruce_b (refs approved 2026-10-13, forest ref-1: "undergrowth of ferns, shrubs
and saplings"): the approved reference, textured headless EEVEE stills (still.py) beside sapling_spruce,
tree_forest_c and a 1.7 m figure, kit-scenario shots (`--tilt off`) from saplingb-kit.json on the
undergrowth forest (-3, 2), and play-scenario shots (`--tilt off`) from saplingb-play.json taken with
docs/checkpoints/sapling_spruce_b/integration.patch applied (both sapling variants drawn by the pine
forest's sapling rule). Writes docs/checkpoints/sapling_spruce_b/sheet.jpg.
Run from the repo root after run.sh: python3 docs/checkpoints/sapling_spruce_b/sheet.py"""
import json
import pathlib

from PIL import Image, ImageDraw

ROOT = pathlib.Path(__file__).resolve().parents[3]
DOCS = ROOT / "docs/checkpoints/sapling_spruce_b"
state = json.loads((ROOT / "target/shots/saplingb-play-state.json").read_text())["scatter"]["drawn_by_piece"]
drawn = f"{state.get('sapling_spruce', 0)} sapling_spruce, {state.get('sapling_spruce_b', 0)} sapling_spruce_b"
CELLS = [
    (ROOT / "assets-source/refs/forest/ref-1.jpg", "ref-1 (approved): undergrowth of ferns, shrubs and saplings"),
    (DOCS / "saplingb-lineup.jpg", "EEVEE: sapling_spruce (2.2 m), sapling_spruce_b (1.95 m, 2.1 m wide), "
                                   "tree_forest_c behind, 1.7 m figure"),
    (DOCS / "sapling_spruce_b-eevee-three_quarter.jpg", "sapling_spruce_b, three-quarter (EEVEE)"),
    (DOCS / "sapling_spruce_b-eevee-low_side.jpg", "sapling_spruce_b, low side: level tiers, bent leader (EEVEE)"),
    (DOCS / "saplingb-kit-play.jpg", "KIT, tilt off: undergrowth forest (-3, 2), sapling_spruce_b at [0, 2] "
                                     "among two sapling_spruce and three shrubs"),
    (DOCS / "saplingb-kit-eye.jpg", "KIT, eye level 1.7 m: the undergrowth"),
    (DOCS / "saplingb-kit-sun.jpg", "KIT: sapling_spruce_b close up, sun side"),
    (DOCS / "saplingb-kit-shade.jpg", "KIT: sapling_spruce_b close up, shade side"),
    (DOCS / "saplingb-play.jpg", f"PLAY, patch applied, tilt off: five pine forests (drawn: {drawn})"),
    (DOCS / "saplingb-sun.jpg", "PLAY: about 4 m from +x+z, sun side: sapling_spruce_b at (63.91, 5.02) behind "
                                "the shrub, tile (2, 0)"),
    (DOCS / "saplingb-shade.jpg", "PLAY: about 4 m from -x-z, shade side (toward the sun): sapling_spruce_b at "
                                  "(52.67, -0.02), tile (2, 0)"),
    (DOCS / "saplingb-eye.jpg", "PLAY, walk mode, eye level 1.7 m: sapling_spruce_b at (38.39, -4.81), tile (1, 0)"),
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
out = DOCS / "sheet.jpg"
sheet.save(out, quality=85)
print("wrote", out)
