"""Print each shot's mean and exit 1 when one is black (mean < 0.02), as a sleeping display writes.
Usage: means.py <out-dir> <tag> <suffix> <shot>..."""
import sys

import numpy as np
from PIL import Image

out, tag, sfx, shots = sys.argv[1], sys.argv[2], sys.argv[3], sys.argv[4:]
black = False
for n in shots:
    m = float(np.asarray(Image.open(f"{out}/{tag}-{n}{sfx}.png").convert("RGB"), dtype=np.float64).mean() / 255)
    print(f"{tag}-{n}{sfx}: mean {m:.3f}")
    black |= m < 0.02
sys.exit(1 if black else 0)
