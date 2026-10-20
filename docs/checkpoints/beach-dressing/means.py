"""Print the mean of every PNG in <dir> whose name starts with <prefix>; exit 1 if any is black or
none exist. Usage: python3 means.py <dir> <prefix>"""
import pathlib
import sys

from PIL import Image, ImageStat

d, prefix = pathlib.Path(sys.argv[1]), sys.argv[2]
files = sorted(d.glob(f"{prefix}*.png"))
bad = not files
for f in files:
    m = sum(ImageStat.Stat(Image.open(f).convert("RGB")).mean) / 3
    print(f"{f.name}: mean {m:.1f}")
    bad |= m < 3
sys.exit(1 if bad else 0)
