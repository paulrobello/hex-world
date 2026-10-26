"""Pixel difference between two sets of lossless shots of the same cameras.

  python3 docs/checkpoints/road_bend_60-trim/shot_diff.py <dir a> <dir b> [<crop out dir>]

For every `<name>.png` in both dirs, prints the largest per-channel difference, the share of pixels
off by more than 8 levels and the bounding box of those pixels. With a crop dir, writes a 3x crop
of each pair around that box (a, b and the difference x8 side by side) for frames that differ.
"""
import pathlib
import sys

import numpy as np
from PIL import Image

a_dir, b_dir = pathlib.Path(sys.argv[1]), pathlib.Path(sys.argv[2])
crops = pathlib.Path(sys.argv[3]) if len(sys.argv) > 3 else None
if crops:
    crops.mkdir(parents=True, exist_ok=True)
for pa in sorted(a_dir.glob("*.png")):
    pb = b_dir / pa.name
    if not pb.exists():
        continue
    a = np.asarray(Image.open(pa).convert("RGB")).astype(int)
    b = np.asarray(Image.open(pb).convert("RGB")).astype(int)
    d = np.abs(a - b).max(axis=2)
    off = d > 8
    line = f"{pa.stem}: max {int(d.max())}, > 8 levels {100 * off.mean():.3f}%"
    if off.any():
        ys, xs = np.nonzero(off)
        box = (int(xs.min()), int(ys.min()), int(xs.max()) + 1, int(ys.max()) + 1)
        line += f", box {box}"
        if crops:
            pad = 20
            x0, y0 = max(0, box[0] - pad), max(0, box[1] - pad)
            x1, y1 = min(a.shape[1], box[2] + pad), min(a.shape[0], box[3] + pad)
            tiles = [a[y0:y1, x0:x1], b[y0:y1, x0:x1], np.clip(np.abs(a - b)[y0:y1, x0:x1] * 8, 0, 255)]
            row = np.concatenate([t.astype(np.uint8) for t in tiles], axis=1)
            img = Image.fromarray(row)
            scale = max(1, min(3, 1200 // max(1, img.width)))
            img.resize((img.width * scale, img.height * scale), Image.NEAREST).save(crops / f"{pa.stem}.png")
    print(line)
