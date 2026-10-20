"""Review sheet for bush_verge_c (refs approved 2026-10-13, road_straight ref-1: "grass verges with tufts and
small bushes"): the approved reference, textured headless EEVEE stills (still.py) beside shrub_forest,
bush_verge, bush_verge_b and a 1.7 m figure, kit-scenario shots (`--tilt off`) from vergec-kit.json on
road tile (0, 0), and play-scenario shots (`--tilt off`) from vergec-play.json taken with
docs/checkpoints/bush_verge_c/integration.patch applied (all three verge variants drawn by the road
tiles' verge rule). Writes docs/checkpoints/bush_verge_c/sheet.jpg.
Run from the repo root after run.sh: python3 docs/checkpoints/bush_verge_c/sheet.py"""
import pathlib

from PIL import Image, ImageDraw

ROOT = pathlib.Path(__file__).resolve().parents[3]
DOCS = ROOT / "docs/checkpoints/bush_verge_c"
CELLS = [
    (ROOT / "assets-source/refs/road_straight/ref-1.jpg", "ref-1 (approved): small bushes on each verge, one with pale flecks"),
    (DOCS / "vergec-lineup.jpg", "EEVEE: shrub_forest, bush_verge (0.88 m), bush_verge_b (0.65 m), bush_verge_c "
                                 "(1.05 m tall, about 1.0 m across), 1.7 m figure"),
    (DOCS / "vergec-trio.jpg", "EEVEE: bush_verge, bush_verge_b and bush_verge_c side by side on a verge"),
    (DOCS / "vergec-trio-far.jpg", "EEVEE: the same three from about 48 m (deep mips): c reads pale-flecked"),
    (DOCS / "bush_verge_c-eevee-three_quarter.jpg", "bush_verge_c, three-quarter (EEVEE): cream blossom corymbs"),
    (DOCS / "bush_verge_c-eevee-low_side.jpg", "bush_verge_c, low side: leaves to the grass, sprayed outline (EEVEE)"),
    (DOCS / "vergec-kit-scale.jpg", "KIT, tilt off: road tile (0, 0), bush_verge_c (right) with bush_verge, "
                                    "bush_verge_b, shrub_forest and a sedan"),
    (DOCS / "vergec-kit-eye.jpg", "KIT, eye level 1.7 m: bush_verge_c in front of bush_verge, bush_verge_b and the sedan"),
    (DOCS / "vergec-kit-sun.jpg", "KIT: close up, sun side (about 4 m)"),
    (DOCS / "vergec-kit-shade.jpg", "KIT: close up, shade side, toward the sun"),
    (DOCS / "vergec-wind-a.jpg", "KIT: wind pair, frame A"),
    (DOCS / "vergec-wind-b.jpg", "KIT: wind pair, frame B (60 fixed frames = 1 s later)"),
    (DOCS / "vergec-play.jpg", "PLAY, patch applied, tilt off: road tiles at play distance"),
    (DOCS / "vergec-mid.jpg", "PLAY: the south verge of tiles (0, 1) and (-1, 1) from 15 m up"),
    (DOCS / "vergec-car.jpg", "PLAY: straight and bend verges with a van on the road for scale"),
    (DOCS / "vergec-sun.jpg", "PLAY: close up, sun side: bush_verge_c at (11.45, 29.31), tile (0, 1)"),
    (DOCS / "vergec-shade.jpg", "PLAY: close up, shade side, toward the sun: the same bush"),
    (DOCS / "vergec-eye.jpg", "PLAY, walk mode, eye level 1.7 m: bush_verge_c at (11.45, 29.31) ahead"),
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
