"""Print each shot's mean (0-1) and exit 0 when any is black (missing, or mean < 0.02), else 1.
Usage: python3 docs/checkpoints/shrub_forest_c/means.py <shot dir> <name>..."""
import pathlib
import sys

from PIL import Image, ImageStat

out = pathlib.Path(sys.argv[1])
black = False
for name in sys.argv[2:]:
    path = out / f"{name}.png"
    if not path.exists():
        print(f"{name}.png missing")
        black = True
        continue
    mean = sum(ImageStat.Stat(Image.open(path).convert("RGB")).mean) / 3 / 255
    print(f"{name}.png mean {mean:.3f}")
    black |= mean < 0.02
sys.exit(0 if black else 1)
