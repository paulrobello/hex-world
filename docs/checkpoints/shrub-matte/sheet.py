"""Before/after review sheet for the woodland shrubs' matte leaves (owner A23 Q2, 2026-10-26).

Run from the repo root after both shot runs of shrub-matte-kit.json and shrub-matte-play.json (README.md):
python3 docs/checkpoints/shrub-matte/sheet.py <before dir> <after dir>
Each row is one shrub at the same camera and fixed clock, cropped and enlarged: before | after.
"""
import sys
from pathlib import Path

from PIL import Image, ImageDraw

OUT = Path("docs/checkpoints/shrub-matte")
# (frame, crop box in the 1600x1000 frame (6:5), label)
ROWS = [
    ("matte-hazel-sun", (600, 360, 1000, 693), "shrub_forest (hazel), kit undergrowth, sun side"),
    ("matte-bramble-sun", (600, 350, 1050, 725), "shrub_forest_c (bramble), kit undergrowth, sun side"),
    ("matte-verge-eye", (680, 430, 1000, 697), "shrub_forest beside the verge bushes, eye level 1.7 m"),
    ("matte-play-sun", (620, 400, 980, 700), "shrub_forest_c in a play forest"),
]
W, H = 600, 500


def main(before: Path, after: Path) -> None:
    sheet = Image.new("RGB", (2 * W + 30, len(ROWS) * (H + 40) + 10), (24, 24, 24))
    d = ImageDraw.Draw(sheet)
    for i, (name, box, label) in enumerate(ROWS):
        y = 10 + i * (H + 40)
        d.text((10, y), f"{label}: before (ORM G 0.50-0.64) | after (0.76-0.86)", fill=(230, 230, 230))
        for j, src in enumerate((before, after)):
            crop = Image.open(src / f"{name}.jpg").crop(box).resize((W, H), Image.LANCZOS)
            sheet.paste(crop, (10 + j * (W + 10), y + 20))
            crop.save(OUT / f"{name}-{'before' if j == 0 else 'after'}-crop.jpg", quality=88)
    sheet.save(OUT / "sheet.jpg", quality=88)
    print(f"wrote {OUT / 'sheet.jpg'}")


if __name__ == "__main__":
    main(Path(sys.argv[1]), Path(sys.argv[2]))
