"""Before/after review sheet for the matte oaks, ferns and dune grass (owner A35 O1, 2026-10-27).

Run from the repo root after both runs of foliage-matte-kit.json (README.md):
python3 docs/checkpoints/foliage-matte/sheet.py <before dir> <after dir>
Each row is one subject at the same camera and fixed clock, cropped and enlarged: before | after.
"""
import sys
from pathlib import Path

from PIL import Image, ImageDraw

OUT = Path("docs/checkpoints/foliage-matte")
# (frame, crop box in the 1600x1000 frame (6:5), label)
ROWS = [
    ("fm-oak-play", (400, 150, 1200, 817), "tree_oak, _b, _c at play distance (kit oak grove)"),
    ("fm-oak-close", (500, 250, 1100, 750), "tree_oak close, sun side"),
    ("fm-hero-oak-play", (500, 150, 1100, 650), "tree_hero_oak at play distance"),
    ("fm-hero-oak-close", (400, 200, 1200, 867), "tree_hero_oak close, sun side"),
    ("fm-fern-play", (500, 250, 1100, 750), "fern_clump at play distance (kit forest floor)"),
    ("fm-fern-close", (500, 300, 1100, 800), "fern_clump close, sun side"),
    ("fm-dune-play", (500, 300, 1100, 800), "beach_dunegrass, _b, _c at play distance (kit beach)"),
    ("fm-dune-close", (500, 300, 1100, 800), "beach_dunegrass close"),
]
W, H = 600, 500


def main(before: Path, after: Path) -> None:
    sheet = Image.new("RGB", (2 * W + 30, len(ROWS) * (H + 40) + 10), (24, 24, 24))
    d = ImageDraw.Draw(sheet)
    for i, (name, box, label) in enumerate(ROWS):
        y = 10 + i * (H + 40)
        d.text((10, y), f"{label}: before | after (leaf ORM G under 0.7 onto 0.76-0.86)", fill=(230, 230, 230))
        for j, src in enumerate((before, after)):
            crop = Image.open(src / f"{name}.jpg").crop(box).resize((W, H), Image.LANCZOS)
            sheet.paste(crop, (10 + j * (W + 10), y + 20))
            crop.save(OUT / f"{name}-{'before' if j == 0 else 'after'}-crop.jpg", quality=88)
    sheet.save(OUT / "sheet.jpg", quality=88)
    print(f"wrote {OUT / 'sheet.jpg'}")


if __name__ == "__main__":
    main(Path(sys.argv[1]), Path(sys.argv[2]))
