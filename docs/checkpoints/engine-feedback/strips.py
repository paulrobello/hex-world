"""Composes the engine feedback captures into labelled frame strips for the owner's review.

Usage: python3 docs/checkpoints/engine-feedback/strips.py <frames dir>
The frames come from `make shots-piece SCRIPT=docs/checkpoints/engine-feedback/engine-feedback.json
OUT=<frames dir> ARGS="--scenario play --clock fixed --water_clock fixed --size 960x600"`. Each strip
labels every frame with its time after the command, from the fixed 1/60 s clock.
"""

import sys
from pathlib import Path

from PIL import Image, ImageChops, ImageDraw, ImageFont, ImageStat

FRAME_MS = 1000 / 60
HERE = Path(__file__).parent
# (strip, frames to show, frame step in the script, crop box in the 960x600 capture, caption)
STRIPS = [
    ("ghost-turn", range(0, 14, 1), 1, "R: the windmill ghost turns 60° over 180 ms (--t) on the Legend curve"),
    ("ghost-in", range(0, 12, 1), 2, "A new slot: the ghost waits 150 ms, then fades in over 120 ms (--t-fast)"),
    ("refused", range(0, 16, 1), 1, "A refused placement: the ghost's edges pulse once over 240 ms (--t-slow)"),
    ("rise", range(0, 24, 1), 6, "Placement: the 80 ms commit flash, then the staged rise (2 s, §4.4)"),
    ("bulldoze-mode", range(0, 14, 1), 1, "Bulldoze on: the slot ring leaves and the target arrives, both 180 ms (--t)"),
    ("bulldoze", range(0, 24, 1), 6, "Bulldoze: a 700 ms ease-in sink inside the dust cloud"),
]
CROP = (180, 60, 780, 480)
COLS = 8
W = 300


def label(img: Image.Image, text: str) -> Image.Image:
    out = Image.new("RGB", (img.width, img.height + 22), (14, 15, 16))
    out.paste(img, (0, 0))
    ImageDraw.Draw(out).text((6, img.height + 4), text, fill=(238, 243, 246), font=ImageFont.load_default())
    return out


def strip(frames: Path, name: str, idx: range, step: int, caption: str, prefix: str) -> tuple[Path, int]:
    paths = [frames / f"{prefix}{name}-{i:02d}.jpg" for i in idx]
    imgs = [Image.open(p).convert("RGB").crop(CROP) for p in paths]
    # Frames that differ from the one before, so a strip that never moves is visible in the report.
    moving = sum(
        1 for a, b in zip(imgs, imgs[1:]) if max(ImageStat.Stat(ImageChops.difference(a, b)).mean) > 0.4
    )
    h = round(imgs[0].height * W / imgs[0].width)
    cells = [label(im.resize((W, h)), f"{round(i * step * FRAME_MS)} ms") for i, im in zip(idx, imgs)]
    rows = (len(cells) + COLS - 1) // COLS
    sheet = Image.new("RGB", (COLS * W, rows * (h + 22) + 26), (14, 15, 16))
    for n, c in enumerate(cells):
        sheet.paste(c, ((n % COLS) * W, (n // COLS) * (h + 22)))
    tag = "Reduced motion. " if prefix else ""
    ImageDraw.Draw(sheet).text((6, rows * (h + 22) + 6), tag + caption, fill=(238, 243, 246), font=ImageFont.load_default())
    out = HERE / f"{prefix}{name}.jpg"
    sheet.save(out, quality=85)
    return out, moving


def main() -> None:
    frames = Path(sys.argv[1])
    for prefix in ("", "rm-"):
        for name, idx, step, caption in STRIPS:
            out, moving = strip(frames, name, idx, step, caption, prefix)
            print(f"{out.name}: {moving} of {len(idx) - 1} steps change the picture")


if __name__ == "__main__":
    main()
