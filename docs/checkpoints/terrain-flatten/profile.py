"""Height profiles across a tile border, before and after the flatten (owner D6-Q1, 2026-10-20).

1. Relief mean by distance from the border (bands, m) for each base, from the old and the new formula.
2. The cross-border profile between two meadow tiles side by side (same rotation): mean height over
   the middle 12 m of the shared edge at each distance from the border on both sides, plus the
   border-normal rise slope (mean, and the share of border length steeper than 3 degrees, the dusk
   sun's elevation).
3. The drawn LOD0 caps (the staged, uncompressed GLBs in target/kit-stage): vertex mean height by
   band, as a check that the meshes carry the new relief.
Usage: python3 docs/checkpoints/terrain-flatten/profile.py"""
import json
import math
import pathlib
import struct
import sys

import numpy as np

ROOT = pathlib.Path(__file__).resolve().parents[3]
sys.path.insert(0, str(ROOT / "scripts/blender"))
sys.path.insert(0, str(ROOT / "scripts/blender/kit"))
import _terrain  # noqa: E402
from lib import hexgeo  # noqa: E402

AP = hexgeo.APOTHEM
BANDS = [(0, 0.5), (0.5, 1), (1, 1.5), (1.5, 2), (2, 3), (3, 4), (4, 6), (6, 14)]
TAN3 = math.tan(math.radians(3))


def ss(t):
    t = min(max(t, 0.0), 1.0)
    return t * t * (3 - 2 * t)


def d_(x, y):
    return hexgeo.dist_to_border(x, y)


OLD = {
    "meadow": lambda x, y: -0.25 * ss(d_(x, y) / 2) * _terrain.undulation(x, y),
    "forest": lambda x, y: -ss(d_(x, y) / 2) * (0.15 * _terrain.undulation(x, y) + 0.1 * _terrain.mounds(x, y)),
    "river meadow part (0.10)": lambda x, y: -0.10 * ss(d_(x, y) / 2) * _terrain.undulation(x, y),
}
NEW = {
    "meadow": _terrain.meadow_relief,
    "forest": _terrain.forest_relief,
    "river meadow part (0.10)": lambda x, y: -0.10 * ss(d_(x, y) / 2) * _terrain.hollow(_terrain.undulation(x, y)),
}
G = np.linspace(-hexgeo.HEX_R, hexgeo.HEX_R, 321)
PTS = [(x, y) for x in G for y in G if d_(x, y) >= 0]


def bands(f):
    zs = np.array([f(x, y) for x, y in PTS])
    ds = np.array([d_(x, y) for x, y in PTS])
    return [float(zs[(ds >= a) & (ds < b)].mean()) for a, b in BANDS], float(zs.mean())


def fmt(v):
    return " ".join(f"{x:+.3f}" for x in v)


print("1. relief mean (m) by distance from the border, bands " + " ".join(f"[{a},{b})" for a, b in BANDS))
for name in OLD:
    (bo, mo), (bn, mn) = bands(OLD[name]), bands(NEW[name])
    print(f"  {name}\n    before {fmt(bo)}   mean {mo:+.4f}\n    after  {fmt(bn)}   mean {mn:+.4f}")

print("2. across the edge-0 border between two meadow tiles (A's edge 0 against B's edge 3), mean over |s| < 6 m")
ts = [-4, -3, -2, -1.5, -1, -0.5, 0, 0.5, 1, 1.5, 2, 3, 4]  # signed distance: < 0 in A, > 0 in B
ss_ = np.linspace(-6, 6, 121)
for tag, f in (("before", OLD["meadow"]), ("after", NEW["meadow"])):
    row = []
    for t in ts:
        if t <= 0:
            row.append(np.mean([f(AP + t, s) for s in ss_]))
        else:
            row.append(np.mean([f(-AP + t, s) for s in ss_]))  # B's own frame: its edge 3 at x = -AP
    print(f"  {tag:6s} " + " ".join(f"{t:+.1f}:{z:+.3f}" for t, z in zip(ts, row)))

print("   border-normal rise toward the border over 0-3 m, all six edges, |s| < 7 m:")
for name in OLD:
    for tag, f in (("before", OLD[name]), ("after", NEW[name])):
        rises, steep = [], []
        for k in range(6):
            n = hexgeo.edge_normal(k)
            t = (-n[1], n[0])
            for s in np.linspace(-7, 7, 71):
                prof = [f((AP - dd) * n[0] + s * t[0], (AP - dd) * n[1] + s * t[1]) for dd in np.linspace(0, 3, 61)]
                r = -np.gradient(prof, 0.05)
                rises.append(r.mean())
                steep.append(r.max() > TAN3)
        print(f"     {name:26s} {tag:6s} mean {math.degrees(math.atan(np.mean(rises))):+.2f} deg, "
              f"> 3 deg on {100 * np.mean(steep):.1f}% of the border")


def glb_lod0_positions(path, node):
    """Positions of mesh `node` in an uncompressed GLB (Blender frame: runtime (x, y, z) -> (x, -z, y))."""
    b = path.read_bytes()
    jl = struct.unpack_from("<I", b, 12)[0]
    j = json.loads(b[20:20 + jl])
    bin0 = 20 + jl + 8
    mesh = next(m for m in j["meshes"] if m["name"] == node)
    out = []
    for prim in mesh["primitives"]:
        acc = j["accessors"][prim["attributes"]["POSITION"]]
        bv = j["bufferViews"][acc["bufferView"]]
        off = bin0 + bv.get("byteOffset", 0) + acc.get("byteOffset", 0)
        a = np.frombuffer(b, dtype=np.float32, count=acc["count"] * 3, offset=off).reshape(-1, 3)
        out.append(a)
    p = np.concatenate(out)
    return np.stack([p[:, 0], -p[:, 2], p[:, 1]], axis=1)


print("3. drawn LOD0 caps (vertex mean z by band, top-facing cap vertices above -0.3 m)")
for piece in ("meadow_base", "forest_base"):
    g = ROOT / "target/kit-stage" / piece / f"{piece}.glb"
    if not g.exists():
        print(f"  {piece}: no staged GLB")
        continue
    p = glb_lod0_positions(g, f"{piece}_lod0")
    p = p[p[:, 2] > -0.3]
    ds = np.array([d_(x, y) for x, y, _ in p])
    print(f"  {piece}: " + " ".join(f"{p[(ds >= a) & (ds < b), 2].mean():+.3f}" for a, b in BANDS)
          + f"   min {p[:, 2].min():+.3f} max {p[:, 2].max():+.3f}")
