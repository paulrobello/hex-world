"""The walker obstacle for massif_granite: the rock's measured top-down outline (form_measure.py's
`footRadiusPer5Deg`, the outermost rock along each 5-degree bearing from the anchor) as convex fans
from the anchor, in the tile frame (runtime x, z; Blender y = -z), printed as the Rust rows of
`walk.rs`'s `FOOTPRINT_OBSTACLES` (the walker's `Obstacle` is a convex polygon).

Run: python3 docs/checkpoints/massif_granite/walk_parts.py <form-measure.json>
"""
import json
import math
import sys

measure = json.load(open(sys.argv[1]))
radii = measure["footRadiusPer5Deg"]
n = len(radii)
pts = [(r * math.cos(math.radians(5 * k)), -r * math.sin(math.radians(5 * k))) for k, r in enumerate(radii)]


def cross(o, a, b):
    return (a[0] - o[0]) * (b[1] - o[1]) - (a[1] - o[1]) * (b[0] - o[0])


def convex(poly):
    m = len(poly)
    signs = [cross(poly[i], poly[(i + 1) % m], poly[(i + 2) % m]) for i in range(m)]
    return all(s <= 1e-6 for s in signs) or all(s >= -1e-6 for s in signs)


start = min(range(n), key=lambda k: radii[k])
parts, k, done = [], start, 0
while done < n:
    chain = [pts[k % n], pts[(k + 1) % n]]
    j = 1
    while done + j < n and convex([(0.0, 0.0)] + chain + [pts[(k + j + 1) % n]]):
        chain.append(pts[(k + j + 1) % n])
        j += 1
    parts.append([(0.0, 0.0)] + chain)
    done += j
    k += j
# Exactness: the fans tile the star polygon, so their areas sum to its area.
star = 0.5 * abs(sum(pts[i][0] * pts[(i + 1) % n][1] - pts[(i + 1) % n][0] * pts[i][1] for i in range(n)))
area = sum(0.5 * abs(sum(p[i][0] * p[(i + 1) % len(p)][1] - p[(i + 1) % len(p)][0] * p[i][1] for i in range(len(p))))
           for p in parts)
assert abs(area - star) < 1e-3, (area, star)
assert all(convex(p) for p in parts)
print(f"// {len(parts)} convex parts, {star:.0f} m² (the rock's top-down cover measures {measure['rockM2']:.0f} m²)")
for p in parts:
    print("    &[" + ", ".join(f"[{x:.2f}, {z:.2f}]" for x, z in p) + "],")
