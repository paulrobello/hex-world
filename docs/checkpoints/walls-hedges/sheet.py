"""B2 walls-and-hedges review sheet: the approved references, headless EEVEE stills beside a 1.7 m figure
and the engine shots, captioned, into docs/checkpoints/walls-hedges/sheet.jpg. The stills it uses are
also written beside it as eevee-*.jpg (JPEG q85).
Usage: python3 docs/checkpoints/walls-hedges/sheet.py <eevee still dir>
(the stills from still.py; the engine shots are the JPEGs make shots-piece wrote here and in draws/)."""
import pathlib
import sys

from PIL import Image, ImageDraw, ImageFont

HERE = pathlib.Path(__file__).resolve().parent
ROOT = HERE.parents[2]
stills = pathlib.Path(sys.argv[1])
CELL_W, CELL_H, CAP = 640, 400, 34
REFS = ROOT / "assets-source/refs/walls_hedges"
STILLS = [
    ("wall_end-eevee-three_quarter", "EEVEE: wall_end, square end at the corner, 1.7 m figure"),
    ("hedge_end-eevee-three_quarter", "EEVEE: hedge_end, rounded at the corner, 1.7 m figure"),
    ("wall_run-eevee-eye", "EEVEE: wall_run at 1.7 m (1.1 m tall, 0.7 m foot)"),
    ("wall_gate-eevee-eye", "EEVEE: wall_gate, closed five-bar gate"),
    ("wall_bend-eevee-top", "EEVEE: wall_bend from above (120 degrees)"),
    ("wall_junction-eevee-three_quarter", "EEVEE: wall_junction (built, never placed by O1)"),
    ("hedge_run-eevee-eye", "EEVEE: hedge_run at 1.7 m (1.9 m tall, 1.5 m foot)"),
    ("hedge_bend-eevee-top", "EEVEE: hedge_bend from above (120 degrees)"),
    ("hedge_oak-eevee-three_quarter", "EEVEE: hedge_oak with tree_oak_b at its pivot"),
]
TILES = [
    (REFS / "ref-1.jpg", "approved ref-1: walls, cope, gate (look; corners rebuilt at 120)"),
    (REFS / "ref-2.jpg", "approved ref-2: hedges, hedgerow oak"),
    (REFS / "ref-4.jpg", "approved ref-4: wall and hedge on a farm edge"),
    *[(HERE / f"eevee-{n}.jpg", c) for n, c in STILLS],
    (HERE / "wh-play.jpg", "engine: the review farm at play distance (seed 1)"),
    (HERE / "draws/wh-farm-overview.jpg", "engine: the 37-tile farm world at the overview"),
    (HERE / "wh-gate.jpg", "engine: the gate, from the yard"),
    (HERE / "wh-bend.jpg", "engine: a 120 degree wall bend at a corner"),
    (HERE / "wh-mixed-corner.jpg", "engine: a wall end meets a hedge end"),
    (HERE / "wh-hedge-sun.jpg", "engine: hedge on the farm's oak edge"),
    (HERE / "wh-eye-hedge.jpg", "engine: eye level (walk mode), hedge"),
    (HERE / "wh-eye-wall.jpg", "engine: eye level (walk mode), wall"),
    (HERE / "wh-eye-gate.jpg", "engine: eye level at the shut gate (it blocks)"),
]

for name, _ in STILLS:
    src = stills / f"{name}.png"
    if src.exists():
        with Image.open(src) as im:
            im = im.convert("RGB")
            if "lineup" in name:  # the lineups sit in a band across the middle of the frame
                im = im.crop((0, 380, im.width, 560))
            im.save(HERE / f"eevee-{name}.jpg", quality=85)
COLS = 3
tiles = [(p, c) for p, c in TILES if p.exists()]
missing = [str(p) for p, _ in TILES if not p.exists()]
rows = (len(tiles) + COLS - 1) // COLS
sheet = Image.new("RGB", (COLS * CELL_W, rows * (CELL_H + CAP)), (24, 24, 24))
draw = ImageDraw.Draw(sheet)
try:
    font = ImageFont.truetype("/System/Library/Fonts/Helvetica.ttc", 20)
except OSError:
    font = ImageFont.load_default()
for i, (path, cap) in enumerate(tiles):
    x, y = (i % COLS) * CELL_W, (i // COLS) * (CELL_H + CAP)
    with Image.open(path) as im:
        im = im.convert("RGB")
        im.thumbnail((CELL_W, CELL_H))
        sheet.paste(im, (x + (CELL_W - im.width) // 2, y + CAP + (CELL_H - im.height) // 2))
    draw.text((x + 8, y + 7), cap, fill=(230, 230, 230), font=font)
sheet.save(HERE / "sheet.jpg", quality=85)
print("wrote", HERE / "sheet.jpg", f"{len(tiles)} tiles", "missing:", missing)
