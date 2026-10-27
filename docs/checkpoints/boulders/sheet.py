"""Review sheet for the B1 granite set (boulder_granite, _b, _c, _d and outcrop_granite; refs
approved A25): the approved references, textured headless EEVEE stills beside rock_small and
rock_small_b, and in-engine shots (`--tilt off`) from boulders-kit.json (the kit meadow at (-2, 0)),
boulders-play.json (meadow, forest and oak tiles in play, walk mode at eye level) and
boulders-coast.json (the beach-ocean-scatter coast world). From the repo root, with
docs/checkpoints/boulders/integration.patch applied and the five pieces staged (make kit-build):
  Blender -b --factory-startup --python-exit-code 1 -P docs/checkpoints/boulders/still.py -- <dir>
  (then each still to <name>.jpg here)
  make shots-piece SCRIPT=docs/checkpoints/boulders/boulders-kit.json OUT=docs/checkpoints/boulders
  make shots-piece SCRIPT=docs/checkpoints/boulders/boulders-play.json OUT=docs/checkpoints/boulders \
    ARGS="--scenario play --clock fixed --water_clock fixed"   (boulders-coast.json the same way)
  python3 docs/checkpoints/boulders/sheet.py"""
import pathlib

from PIL import Image, ImageDraw

ROOT = pathlib.Path(__file__).resolve().parents[3]
DOCS = ROOT / "docs/checkpoints/boulders"
REFS = ROOT / "assets-source/refs/boulders"
CELLS = [
    (REFS / "ref-1.jpg", "ref-1 (approved A25): the four boulders"),
    (REFS / "ref-2.jpg", "ref-2 (approved A25): the whaleback outcrop with sheet joints"),
    (DOCS / "boulders-lineup.jpg", "EEVEE: rock_small, rock_small_b, then boulder_granite, _b, _c, _d, outcrop; 1.7 m figure"),
    (DOCS / "outcrop_granite-eevee-three_quarter.jpg", "outcrop_granite three-quarter (EEVEE), joints and slab steps"),
    (DOCS / "boulder_granite_d-eevee-three_quarter.jpg", "boulder_granite_d three-quarter (EEVEE)"),
    (DOCS / "boulder_granite_c-eevee-three_quarter.jpg", "boulder_granite_c, the wedge (EEVEE)"),
    (DOCS / "b1-kit-play.jpg", "in engine, kit meadow at (-2, 0) from play distance, tilt off"),
    (DOCS / "b1-kit-sun.jpg", "boulder_granite_d, sun side (about 6 m)"),
    (DOCS / "b1-kit-shade.jpg", "boulder_granite_d, shade side (about 6 m)"),
    (DOCS / "b1-kit-outcrop.jpg", "outcrop_granite close (about 9 m)"),
    (DOCS / "b1-kit-small.jpg", "boulder_granite and _b close"),
    (DOCS / "b1-kit-eye.jpg", "eye level 1.7 m across the boulder meadow"),
    (DOCS / "b1-play.jpg", "play: meadows, pine forests and oak tiles with the boulder scatter"),
    (DOCS / "b1-play-oak-scale.jpg", "play: scale beside the hero oak"),
    (DOCS / "b1-play-walk-eye.jpg", "play: walk mode, eye level 1.7 m"),
    (DOCS / "b1-coast-overview.jpg", "play: the coast world from above; the boulders and outcrops shown stand on its inland meadows"),
    (DOCS / "b1-coast-south.jpg", "play: the south beaches (no coast tile here keeps a boulder under the 0-6 cap)"),
    (DOCS / "b1-coast-north.jpg", "play: the north beaches; beach (0, -1), centre front, keeps one boulder on its grass-to-sand band"),
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
