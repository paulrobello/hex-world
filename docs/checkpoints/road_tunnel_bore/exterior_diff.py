"""Pixel diff of the closed road_tunnel_portal before and after the open-tunnel extension (_tunnel_portal_open.py), and
of the new render against the approved sheet's still. Both renders come from docs/checkpoints/road_tunnel_portal/
eevee_still.py on the same stand-in (its cover skirt closes edge 0 as before), once with the committed geometry
rebuilt without _tunnel_portal_open and once with the open build, the same baked maps.
Usage: python3 docs/checkpoints/road_tunnel_bore/exterior_diff.py <before_dir> <after_dir> [name] (needs Pillow,
numpy); writes docs/checkpoints/road_tunnel_bore/<name, default exterior-diff>.json and prints it."""
import json
import pathlib
import sys

import numpy as np
from PIL import Image

ROOT = pathlib.Path(__file__).resolve().parents[3]
APPROVED = ROOT / "docs/checkpoints/road_tunnel_portal"
before, after = pathlib.Path(sys.argv[1]), pathlib.Path(sys.argv[2])


def load(p):
    return np.asarray(Image.open(p).convert("RGB")).astype(np.int16)


def stats(a, b):
    d = np.abs(a - b).max(axis=2)
    return {"meanAbs": round(float(np.abs(a - b).mean()), 4), "max": int(d.max()),
            "pixelsOver8": int((d > 8).sum()), "pixelsOver24": int((d > 24).sum()), "pixels": int(d.size)}


out = {}
for v in ("three_quarter", "front", "eye"):
    a, b = load(before / f"portal-{v}.jpg"), load(after / f"portal-{v}.jpg")
    out[v] = {"beforeVsAfter": stats(a, b)}
    ap = APPROVED / f"portal-{v}.jpg"
    if ap.exists():
        out[v]["approvedVsAfter"] = stats(load(ap), b)
name = sys.argv[3] if len(sys.argv) > 3 else "exterior-diff"
(ROOT / f"docs/checkpoints/road_tunnel_bore/{name}.json").write_text(json.dumps(out, indent=2) + "\n")
print(json.dumps(out, indent=2))
