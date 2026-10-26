"""Before/after review sheet for the verge-bush shine fix (owner, 2026-10-26: "these bushes are too shiny").

Run from the repo root after both shot runs of bush-shine.json (README.md):
python3 docs/checkpoints/bush-shine/sheet.py <before dir> <after dir>
Each row is one bush at the same camera and fixed clock, cropped and doubled: before | after.
"""
import sys
from pathlib import Path

from PIL import Image, ImageDraw

OUT = Path("docs/checkpoints/bush-shine")
# (frame, crop box in the 1600x1000 frame, label)
ROWS = [
    ("bush-shine-near", (950, 250, 1250, 500), "bush_verge, 2.2 m camera on the road"),
    ("bush-shine-blossom", (890, 310, 1190, 560), "bush_verge_c, 2.2 m camera on the road"),
    ("bush-shine-far", (750, 270, 1050, 520), "a second verge bush, 2.2 m camera on the road"),
]
SCALE = 2


def main(before: Path, after: Path) -> None:
    w, h = 300 * SCALE, 250 * SCALE
    sheet = Image.new("RGB", (2 * w + 30, len(ROWS) * (h + 40) + 10), (24, 24, 24))
    d = ImageDraw.Draw(sheet)
    for i, (name, box, label) in enumerate(ROWS):
        y = 10 + i * (h + 40)
        d.text((10, y), f"{label}: before (ORM G 0.50-0.64) | after (0.76-0.86)", fill=(230, 230, 230))
        for j, src in enumerate((before, after)):
            crop = Image.open(src / f"{name}.jpg").crop(box).resize((w, h), Image.LANCZOS)
            sheet.paste(crop, (10 + j * (w + 10), y + 20))
            crop.save(OUT / f"{name}-{'before' if j == 0 else 'after'}-crop.jpg", quality=88)
    sheet.save(OUT / "sheet.jpg", quality=88)
    print(f"wrote {OUT / 'sheet.jpg'}")


if __name__ == "__main__":
    main(Path(sys.argv[1]), Path(sys.argv[2]))
