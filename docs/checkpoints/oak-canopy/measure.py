"""tree_hero_oak canopy gap vs leaf statistics of the shipped maps (albedo, ORM, normal).

Usage: python3 measure.py <dir with tree_hero_oak.{albedo,orm,normal,masks}.png> <reference albedo png> [out.json]

Texels are classified once, on the reference (pre-fix) albedo, so before and after compare the same
texels: canopy = the shell islands (masks.R < 0.9, where the canopy carries snow capacity <= 0.85, in
v < 0.75), eroded 8 texels past the bake gutter; gap = reference albedo within 0.045 (max channel) of
the dark dome tone #1f3812 the leaves sit on; leaf = farther than 0.07 from it (every leaf tone is
>= 0.145 away). Luma is linear Rec. 709 relative luminance of the sRGB albedo.
"""
import json
import sys

import numpy as np
from PIL import Image

P = "tree_hero_oak"


def rgba(path):
    return np.asarray(Image.open(path).convert("RGBA")).astype(np.float32) / 255.0


def lin(x):
    return np.where(x <= 0.04045, x / 12.92, ((x + 0.055) / 1.055) ** 2.4)


d, ref_path = sys.argv[1], sys.argv[2]
alb, orm, nrm, msk = (rgba(f"{d}/{P}.{k}.png") for k in ("albedo", "orm", "normal", "masks"))
ref = rgba(ref_path)[..., :3]
H = alb.shape[0]
canopy = np.zeros(alb.shape[:2], bool)
canopy[int(round(0.25 * H)):] = True  # PNG rows below the leaf strip (v < 0.75)
canopy &= msk[..., 0] < 0.9
for _ in range(8):
    e = canopy.copy()
    e[1:] &= canopy[:-1]
    e[:-1] &= canopy[1:]
    e[:, 1:] &= canopy[:, :-1]
    e[:, :-1] &= canopy[:, 1:]
    canopy = e
dist = np.abs(ref - np.array([0x1F, 0x38, 0x12]) / 255.0).max(-1)
gap = canopy & (dist < 0.045)
leaf = canopy & (dist > 0.07)
rgb = alb[..., :3]
luma = (lin(rgb) * np.array([0.2126, 0.7152, 0.0722])).sum(-1)
nz = nrm[..., 2] * 2 - 1


def st(a, m):
    v = a[m]
    return {"mean": round(float(v.mean()), 4), "p05": round(float(np.percentile(v, 5)), 4),
            "p50": round(float(np.percentile(v, 50)), 4), "p95": round(float(np.percentile(v, 95)), 4)}


out = {"canopyTexels": int(canopy.sum()), "gapTexels": int(gap.sum()), "leafTexels": int(leaf.sum()),
       "gapShare": round(float(gap.sum() / max(canopy.sum(), 1)), 4)}
for name, m in (("gap", gap), ("leaf", leaf)):
    out[name] = {"albedoSrgbMean": [int(round(float(x) * 255)) for x in rgb[m].mean(0)],
                 "albedoLumaLinear": st(luma, m), "roughness": st(orm[..., 1], m), "ao": st(orm[..., 0], m),
                 "metallic": st(orm[..., 2], m), "normalZ": st(nz, m)}
out["leafOverGapLuma"] = round(out["leaf"]["albedoLumaLinear"]["mean"] / out["gap"]["albedoLumaLinear"]["mean"], 2)
print(json.dumps(out, indent=1))
if len(sys.argv) > 3:
    with open(sys.argv[3], "w") as f:
        json.dump(out, f, indent=1)
        f.write("\n")
