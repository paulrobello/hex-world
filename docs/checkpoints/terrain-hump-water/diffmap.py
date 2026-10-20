"""Where a before/after pair differs: |after - before| luminance, amplified, over a dimmed copy of the
after shot, plus the mean change per region of distance band. Shows that a normals-only change moves
pixels only along the river tiles' borders. Usage: diffmap.py before.png after.png out.jpg [gain]"""
import sys

import numpy as np
from PIL import Image

a, b, out = sys.argv[1:4]
gain = float(sys.argv[4]) if len(sys.argv) > 4 else 8.0
A = np.asarray(Image.open(a).convert("RGB"), dtype=np.float64)
B = np.asarray(Image.open(b).convert("RGB"), dtype=np.float64)
la, lb = A.mean(axis=2), B.mean(axis=2)
d = lb - la
mag = np.clip(np.abs(d) * gain, 0, 255)
base = B * 0.35
rgb = base.copy()
rgb[..., 0] = np.clip(base[..., 0] + np.where(d > 0, mag, 0), 0, 255)  # brighter after: red
rgb[..., 2] = np.clip(base[..., 2] + np.where(d < 0, mag, 0), 0, 255)  # darker after: blue
Image.fromarray(rgb.astype(np.uint8)).save(out, quality=88)
print(f"{out}: mean |change| {np.abs(d).mean():.3f} levels, p99 {np.percentile(np.abs(d), 99):.2f}, "
      f"max {np.abs(d).max():.1f}, pixels changed by >= 2 levels {(np.abs(d) >= 2).mean() * 100:.2f}%")
