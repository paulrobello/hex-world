"""Review sheet for the station, rail_station_platform and rail_station_shelter (P3, refs A17): the approved references, a textured headless EEVEE
still, and the in-engine shots (`--tilt off`) from station.json. Run from the repo root after
run.sh: python3 docs/checkpoints/rail_station/sheet.py"""
import pathlib
import sys

ROOT = pathlib.Path(__file__).resolve().parents[3]
sys.path.insert(0, str(ROOT / "scripts/assets"))
from PIL import Image, ImageDraw  # noqa: E402

DOCS = ROOT / "docs/checkpoints/rail_station"
CELLS = [
    (ROOT / "assets-source/refs/rail_station/ref-1.jpg", "ref-1 (approved): the shelter is the house style"),
    (ROOT / "assets-source/refs/rail_station/ref-2.jpg", "ref-2 (approved)"),
    (DOCS / "still.jpg", "textured EEVEE still (Blender AgX; evidence only)"),
    (DOCS / "station-play.jpg", "in engine, play distance, tilt off: station beside the straight, train on the bend"),
    (DOCS / "station-close-sun.jpg", "close-up, sun side (about 6 m)"),
    (DOCS / "station-close-shade.jpg", "close-up, shade side (about 6 m)"),
    (DOCS / "station-eye.jpg", "eye level 1.7 m on the platform, along the track"),
    (DOCS / "station-scale.jpg", "scale: carriage, station, barn and the bakery beyond"),
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
