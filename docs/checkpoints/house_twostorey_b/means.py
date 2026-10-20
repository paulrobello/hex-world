"""Exit 1 if any of the named PNGs is black (mean < 0.02) or missing; print each mean.
Usage: python3 docs/checkpoints/house_twostorey_b/means.py <dir> <name> [<name> ...] (names without .png)."""
import pathlib
import sys

from PIL import Image, ImageStat

d = pathlib.Path(sys.argv[1])
bad = False
for n in sys.argv[2:]:
    p = d / f"{n}.png"
    if not p.exists():
        print(f"{n}: missing")
        bad = True
        continue
    m = sum(ImageStat.Stat(Image.open(p).convert("RGB")).mean) / 3 / 255
    print(f"{n}: mean {m:.3f}")
    bad |= m < 0.02
sys.exit(1 if bad else 0)
