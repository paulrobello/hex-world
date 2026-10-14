"""On-screen gap vs leaf colour in the before/after oak close-ups (same camera, fixed clock).

Usage: python3 docs/checkpoints/oak-canopy/screen.py <label>   (after shoot.sh before and shoot.sh <label>)

Gap pixels are those whose relative luminance Y changed by more than 0.01 between the before and
<label> JPEGs (only the gap texels changed in the maps); leaf pixels are canopy-green pixels (G is the
largest channel, Y > 0.08 before) that changed by less than 0.003. Y is Rec. 709 luminance of
sRGB-decoded values; saturation is HSV (max - min) / max on sRGB values, so a grey sheen reads low.
play-far is cropped to the oak (CROP): flocks fly elsewhere in that view and differ between runs.
Writes docs/checkpoints/oak-canopy/screen-<label>.json.
"""
import json
import pathlib
import sys

import numpy as np
from PIL import Image

DOCS = pathlib.Path(__file__).resolve().parent
VIEWS = ("closeup-sun", "closeup-shade", "closeup-grazing", "play", "play-far")
CROP = {"play-far": (650, 360, 970, 610)}  # x0, y0, x1, y1 pixels at 1600x1000
LABEL = sys.argv[1]


def load(label, view):
    img = Image.open(DOCS / f"oak-canopy-{label}-{view}.jpg").convert("RGB")
    if view in CROP:
        img = img.crop(CROP[view])
    return np.asarray(img).astype(np.float32) / 255


def lum(c):
    lin = np.where(c <= 0.04045, c / 12.92, ((c + 0.055) / 1.055) ** 2.4)
    return (lin * np.array([0.2126, 0.7152, 0.0722])).sum(-1)


def sat(c):
    mx = c.max(-1)
    return np.where(mx > 0, (mx - c.min(-1)) / np.maximum(mx, 1e-6), 0.0)


def summary(c, m):
    return {"srgbMean": [int(round(float(x) * 255)) for x in c[m].mean(0)], "luminance": round(float(lum(c)[m].mean()), 4),
            "saturation": round(float(sat(c)[m].mean()), 3)}


out = {}
for v in VIEWS:
    b, a = load("before", v), load(LABEL, v)
    dy = lum(b) - lum(a)
    gap = dy > 0.01
    green = (b[..., 1] >= b[..., 0]) & (b[..., 1] >= b[..., 2]) & (lum(b) > 0.08)
    leaf = green & (np.abs(dy) < 0.003)
    out[v] = {"gapPixels": int(gap.sum()), "leafPixels": int(leaf.sum())}
    if gap.any():
        out[v]["gapBefore"], out[v]["gapAfter"] = summary(b, gap), summary(a, gap)
    if leaf.any():
        out[v]["leafBefore"], out[v]["leafAfter"] = summary(b, leaf), summary(a, leaf)
    out[v]["meanLuminanceBefore"] = round(float(lum(b).mean()), 4)
    out[v]["meanLuminanceAfter"] = round(float(lum(a).mean()), 4)
print(json.dumps(out, indent=1))
with open(DOCS / f"screen-{LABEL}.json", "w") as f:
    json.dump(out, f, indent=1)
    f.write("\n")
