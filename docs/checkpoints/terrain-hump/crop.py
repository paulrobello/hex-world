"""Stack the same crop of two PNGs (top: a, bottom: b), contrast-stretched alike, into out.
Usage: crop.py a.png b.png out.png x0 y0 x1 y1"""
import sys

import numpy as np
from PIL import Image

a, b, out = sys.argv[1:4]
x0, y0, x1, y1 = map(int, sys.argv[4:8])
ims = [np.asarray(Image.open(p).convert("RGB"), dtype=np.float64)[y0:y1, x0:x1] for p in (a, b)]
lo = min(np.percentile(i, 1) for i in ims)
hi = max(np.percentile(i, 99) for i in ims)
st = [np.clip((i - lo) / (hi - lo) * 255, 0, 255).astype(np.uint8) for i in ims]
Image.fromarray(np.vstack(st)).save(out)
for name, i in zip((a, b), ims):
    g = i.mean(axis=2)
    # Row-to-row luminance variation along the crop (ridge lines show as structure).
    print(name.split("/")[-1], f"mean {g.mean():.1f} std {g.std():.2f}")
