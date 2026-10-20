"""Review sheet for bush_verge_b (refs approved 2026-10-13, road_straight ref-1: "grass verges with tufts and
small bushes", the small pale bush beside the dark round one): the approved reference, textured headless
EEVEE stills (still.py) beside shrub_forest, bush_verge and a 1.7 m figure, kit-scenario shots
(`--tilt off`) from vergeb-kit.json on road tile (0, 0), and play-scenario shots (`--tilt off`) from
vergeb-play.json taken with docs/checkpoints/bush_verge_b/integration.patch applied (both verge variants
drawn by the road tiles' verge rule). Writes docs/checkpoints/bush_verge_b/sheet.jpg.
Run from the repo root after run.sh: python3 docs/checkpoints/bush_verge_b/sheet.py"""
import pathlib

from PIL import Image, ImageDraw

ROOT = pathlib.Path(__file__).resolve().parents[3]
DOCS = ROOT / "docs/checkpoints/bush_verge_b"
CELLS = [
    (ROOT / "assets-source/refs/road_straight/ref-1.jpg", "ref-1 (approved): a dark round and a small pale bush on each verge"),
    (DOCS / "vergeb-lineup.jpg", "EEVEE: shrub_forest (2.06 m, 1.33 m), bush_verge (1.19 m, 0.88 m), bush_verge_b "
                                 "(0.95 x 0.796 m, 0.65 m), 1.7 m figure"),
    (DOCS / "vergeb-pair.jpg", "EEVEE: bush_verge and bush_verge_b side by side on a verge"),
    (DOCS / "bush_verge_b-eevee-three_quarter.jpg", "bush_verge_b, three-quarter (EEVEE)"),
    (DOCS / "bush_verge_b-eevee-low_side.jpg", "bush_verge_b, low side: leaves to the grass, lumpy outline (EEVEE)"),
    (DOCS / "bush_verge_b-eevee-top.jpg", "bush_verge_b, top: uneven, slightly oval plan (EEVEE)"),
    (DOCS / "vergeb-kit-scale.jpg", "KIT, tilt off: road tile (0, 0), bush_verge_b among three bush_verge, "
                                    "shrub_forest and a sedan"),
    (DOCS / "vergeb-kit-eye.jpg", "KIT, eye level 1.7 m: bush_verge_b (centre) between shrub_forest and bush_verge"),
    (DOCS / "vergeb-play.jpg", "PLAY, patch applied, tilt off: road tiles at play distance (state: 8 bush_verge, "
                               "8 bush_verge_b drawn)"),
    (DOCS / "vergeb-car.jpg", "PLAY: straight and bend verges with a van on the road for scale"),
    (DOCS / "vergeb-sun.jpg", "PLAY: close up, sun side (about 6 m): bush_verge_b at (-8.67, 29.08), tile (-1, 1)"),
    (DOCS / "vergeb-shade.jpg", "PLAY: close up, shade side, toward the sun: bush_verge_b at (21.62, 18.30), tile (0, 1)"),
    (DOCS / "vergeb-eye.jpg", "PLAY, walk mode, eye level 1.7 m: bush_verge_b at (-8.67, 29.08) in front"),
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
