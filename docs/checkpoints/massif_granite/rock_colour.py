"""Rock colour and contrast beside ref-2 (brief: shading and colour): luma mean and spread, and the
shares of lichen-green, mustard, dark (moss, joints, shade) and moss (dark, saturated green) pixels
over a crop of the rock face,
in ref-2 and in review_render.py's still at ref-2's fitted camera (same framing, so the same crop
in normalised coordinates holds rock in both).

Run: python3 docs/checkpoints/massif_granite/rock_colour.py <ref2cam still> [more stills...]
"""
import json
import pathlib
import sys

import numpy as np
from PIL import Image

REF = pathlib.Path(__file__).resolve().parents[3] / "assets-source/refs/massif_granite/ref-2.jpg"
CROP = (0.39, 0.22, 0.60, 0.46)  # normalised (x0, y0, x1, y1): the front rock face below the summit


def stats(path):
    im = Image.open(path).convert("RGB")
    w, h = im.size
    a = np.asarray(im.crop((int(CROP[0] * w), int(CROP[1] * h), int(CROP[2] * w), int(CROP[3] * h)))).astype(float) / 255
    r, g, b = a[..., 0], a[..., 1], a[..., 2]
    luma = 0.2126 * r + 0.7152 * g + 0.0722 * b
    mx, mn = a.max(-1), a.min(-1)
    sat = (mx - mn) / np.maximum(mx, 1e-6)
    green = (g > r + 0.015) & (g > b + 0.03) & (sat > 0.08) & (luma > 0.3)
    mustard = (r > b + 0.12) & (g > b + 0.08) & (luma > 0.3)
    dark = luma < 0.3
    moss = (luma < 0.32) & (luma > 0.04) & (g > r + 0.02) & (g > b + 0.04) & (sat > 0.2)
    return {"lumaMean": round(float(luma.mean()), 3), "lumaStd": round(float(luma.std()), 3),
            "lumaP05": round(float(np.percentile(luma, 5)), 3), "lumaP95": round(float(np.percentile(luma, 95)), 3),
            "satMean": round(float(sat.mean()), 3), "lichenGreen": round(float(green.mean()), 3),
            "mustard": round(float(mustard.mean()), 3), "dark": round(float(dark.mean()), 3),
            "moss": round(float(moss.mean()), 3)}


out = {"ref-2": stats(REF)}
for p in sys.argv[1:]:
    out[pathlib.Path(p).name] = stats(p)
print(json.dumps(out, indent=1))
