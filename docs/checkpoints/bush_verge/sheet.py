"""Review sheet for bush_verge (refs approved 2026-10-13, road_straight ref-1: "grass verges with tufts and
small bushes"): the approved reference, textured headless EEVEE stills (still.py) beside shrub_forest and
a 1.7 m figure, kit-scenario shots (`--tilt off`) from verge-kit.json on road tile (0, 0) (kit_scene
VERGE_BUSHES), and play-scenario shots (`--tilt off`) from verge-play.json taken with
docs/checkpoints/bush_verge/integration.patch applied (the verge scatter rule on the road tiles). Writes
docs/checkpoints/bush_verge/sheet.jpg.
Run from the repo root after run.sh: python3 docs/checkpoints/bush_verge/sheet.py"""
import pathlib

from PIL import Image, ImageDraw

ROOT = pathlib.Path(__file__).resolve().parents[3]
DOCS = ROOT / "docs/checkpoints/bush_verge"
CELLS = [
    (ROOT / "assets-source/refs/road_straight/ref-1.jpg", "ref-1 (approved): small dark round bushes on the verges"),
    (DOCS / "verge-lineup.jpg", "EEVEE: shrub_forest (2.06 m wide, 1.33 m), bush_verge (1.19 m, 0.88 m), "
                                "1.7 m figure, by a kerb"),
    (DOCS / "bush_verge-eevee-three_quarter.jpg", "bush_verge, three-quarter (EEVEE)"),
    (DOCS / "bush_verge-eevee-low_side.jpg", "bush_verge, low side: leaves to the grass, no stems (EEVEE)"),
    (DOCS / "verge-kit-scale.jpg", "KIT, tilt off: road tile (0, 0), three bush_verge, shrub_forest (centre) and a "
                                   "sedan on the lane"),
    (DOCS / "verge-kit-eye.jpg", "KIT, eye level 1.7 m: bush_verge (front) beside shrub_forest, sedan behind"),
    (DOCS / "verge-play.jpg", "PLAY, patch applied, tilt off: road_straight at play distance, 4 bushes "
                              "a tile on the verges"),
    (DOCS / "verge-car.jpg", "PLAY: straight and bend verges with a van on the road for scale"),
    (DOCS / "verge-sun.jpg", "PLAY: close up, sun side (about 6 m), beyond the kerb in the mown verge grass"),
    (DOCS / "verge-shade.jpg", "PLAY: close up, shade side, toward the sun (about 6 m)"),
    (DOCS / "verge-eye.jpg", "PLAY, walk mode, eye level 1.7 m along the south verge"),
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
