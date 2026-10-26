"""Compare a trimmed stub's masks bake with road_straight_edge's committed one.

  python3 docs/checkpoints/road_bend_60-trim/masks_diff.py <stub masks png> <side: -1|1>

Both bakes map the stub through the same planar UV over the 2R square (16 px/m: U = (x + R) / 2R,
V = (y + R) / 2R in the piece's Blender frame, PNG row 0 at V = 1), so equal texels cover the same
spot of the stub, and a texel under the trimmed stub showed the straight's value there before the
trim. Side -1 is e0 (cut on -y), +1 is e1 (cut on +y). Prints, per RGBA channel, the largest
difference and the count of texels off by more than 2 levels, over the texels whose centre lies on
the trimmed stub at least `MARGIN` from the cut, and separately over the band within `MARGIN` of the
cut (bilinear and mip footprint of the cut edge).
"""
import math
import pathlib
import sys

import numpy as np
from PIL import Image

ROOT = pathlib.Path(__file__).resolve().parents[3]
R, APOTHEM, SINK_T, JOINT = 16.0, 16.0 * math.sqrt(3.0) / 2.0, 4.6, 6.0
MARGIN = 2.0 / 16.0  # two texels

ref = np.asarray(Image.open(ROOT / "assets/kit/p1/road_straight_edge/road_straight_edge.masks.png").convert("RGBA")).astype(int)
new = np.asarray(Image.open(sys.argv[1]).convert("RGBA")).astype(int)
side = int(sys.argv[2])
n = ref.shape[0]
c = (np.arange(n) + 0.5) / n
xs, ys = np.meshgrid(c * 2 * R - R, (1.0 - c) * 2 * R - R)
on_stub = (xs >= JOINT) & (xs <= APOTHEM) & (np.abs(ys) <= SINK_T)
cut = 0.5 * xs - side * math.sqrt(3.0) / 2.0 * ys  # >= 0 on the kept side, distance to the cut plane
inner = on_stub & (cut >= MARGIN)
band = on_stub & (np.abs(cut) < MARGIN)
for name, sel in (("trimmed stub, >= 2 texels from the cut", inner), ("within 2 texels of the cut", band)):
    print(f"{name}: {int(sel.sum())} texels")
    for i, ch in enumerate("RGBA"):
        d = np.abs(ref[..., i] - new[..., i])[sel]
        print(f"  {ch}: max diff {int(d.max())}, texels off by > 2: {int((d > 2).sum())}, mean {d.mean():.4f}")
