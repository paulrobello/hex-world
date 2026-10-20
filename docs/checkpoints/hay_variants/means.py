"""Mean brightness of each named shot PNG (a black frame reads about 0). Usage:
python3 docs/checkpoints/hay_variants/means.py <png dir> <name> [<name> ...]; exits 1 if any is black."""
import pathlib
import sys

from PIL import Image, ImageStat

d = pathlib.Path(sys.argv[1])
bad = False
for n in sys.argv[2:]:
    m = ImageStat.Stat(Image.open(d / f"{n}.png").convert("L")).mean[0] / 255
    print(f"{n}: mean {m:.3f}")
    bad |= m < 0.02
sys.exit(1 if bad else 0)
