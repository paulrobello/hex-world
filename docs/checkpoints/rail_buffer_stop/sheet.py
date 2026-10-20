"""Build docs/checkpoints/rail_buffer_stop/sheet.jpg: the approved ref-2, the six-view renders
(assets-source/kit/rail_buffer_stop/renders, slot colours: the piece has no textures but masks), and
the in-engine kit-scenario shots (this folder's bufstop-*.jpg, from shots.json and night.json).
Needs Pillow. Run: python3 docs/checkpoints/rail_buffer_stop/sheet.py"""
import pathlib

from PIL import Image, ImageDraw, ImageFont

ROOT = pathlib.Path(__file__).resolve().parents[3]
HERE = pathlib.Path(__file__).parent
RENDERS = ROOT / "assets-source/kit/rail_buffer_stop/renders"
CELL_W, CELL_H, LABEL = 480, 300, 26
COLS = 4

CELLS = [
    (ROOT / "assets-source/refs/rail_buffer_stop/ref-2.jpg", "approved ref-2 (owner D2)"),
    (RENDERS / "three_quarter.jpg", "six views: three-quarter"),
    (RENDERS / "front.jpg", "front"),
    (RENDERS / "back.jpg", "back"),
    (RENDERS / "left.jpg", "left (buffer face)"),
    (RENDERS / "right.jpg", "right (rear)"),
    (RENDERS / "top.jpg", "top"),
    (HERE / "bufstop-night.jpg", "lamp at night (--kit_hooks night)"),
    (HERE / "bufstop-straight-play.jpg", "straight end: play distance"),
    (HERE / "bufstop-straight-sun.jpg", "straight: engine buffered up, sun side"),
    (HERE / "bufstop-straight-rear.jpg", "straight: rear, braces on the rails"),
    (HERE / "bufstop-straight-top.jpg", "straight: feet on the rail heads"),
    (HERE / "bufstop-eye.jpg", "straight: eye level 1.7 m"),
    (HERE / "bufstop-bend-play.jpg", "bend end: play distance"),
    (HERE / "bufstop-bend-sun.jpg", "bend: engine 0.9 m off, 40 deg skew"),
    (HERE / "bufstop-bend-rear.jpg", "bend: rear"),
    (HERE / "bufstop-bend-top.jpg", "bend: feet on the curved rails"),
]


def fit(img):
    img = img.convert("RGB")
    s = min(CELL_W / img.width, CELL_H / img.height)
    img = img.resize((max(1, round(img.width * s)), max(1, round(img.height * s))), Image.LANCZOS)
    cell = Image.new("RGB", (CELL_W, CELL_H), (40, 40, 40))
    cell.paste(img, ((CELL_W - img.width) // 2, (CELL_H - img.height) // 2))
    return cell


def main():
    try:
        font = ImageFont.truetype("/System/Library/Fonts/Helvetica.ttc", 17)
    except OSError:
        font = ImageFont.load_default()
    rows = (len(CELLS) + COLS - 1) // COLS
    sheet = Image.new("RGB", (COLS * CELL_W, rows * (CELL_H + LABEL)), (24, 24, 24))
    draw = ImageDraw.Draw(sheet)
    for i, (path, label) in enumerate(CELLS):
        x, y = (i % COLS) * CELL_W, (i // COLS) * (CELL_H + LABEL)
        sheet.paste(fit(Image.open(path)), (x, y + LABEL))
        draw.text((x + 8, y + 4), label, fill=(230, 230, 230), font=font)
    out = HERE / "sheet.jpg"
    sheet.save(out, quality=85)
    print(f"wrote {out.relative_to(ROOT)} {sheet.size}")


main()
