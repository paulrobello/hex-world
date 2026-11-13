"""The walker obstacles of the single-tile landforms, printed as Rust rows of `walk.rs`'s
`FOOTPRINT_OBSTACLES` parts (the walker's `Obstacle` is a convex polygon, tile frame, runtime x, z).

Input: `glb_visible.json`, the top-down visible surface of the piece's staged LOD0 (a 0.1 m z-buffer:
which slot is on top in each cell, so rock buried under the grass is not counted).

- `islands`: each visible rock island's convex hull, one part each (hill_meadow's slabs).
- `star`: the outermost visible rock along each 5-degree bearing from the measure's centre, as
  convex fans from that centre (massif_granite's walk_parts.py; peak_granite's rock is one island).

Run: python3 docs/checkpoints/hill_meadow/walk_parts.py <visible.json> islands|star
"""
import json
import math
import sys

measure = json.load(open(sys.argv[1]))
mode = sys.argv[2]


def cross(o, a, b):
    return (a[0] - o[0]) * (b[1] - o[1]) - (a[1] - o[1]) * (b[0] - o[0])


def convex(poly):
    m = len(poly)
    signs = [cross(poly[i], poly[(i + 1) % m], poly[(i + 2) % m]) for i in range(m)]
    return all(s <= 1e-6 for s in signs) or all(s >= -1e-6 for s in signs)


def area(p):
    return 0.5 * abs(sum(p[i][0] * p[(i + 1) % len(p)][1] - p[(i + 1) % len(p)][0] * p[i][1] for i in range(len(p))))


def simplify(poly, tol=0.03):
    """Drops hull vertices whose triangle with their neighbours is under `tol` m² (the 0.1 m grid's
    near-collinear steps); dropping a vertex of a convex polygon keeps it convex."""
    poly = list(poly)
    while len(poly) > 3:
        m = len(poly)
        k = min(range(m), key=lambda i: area([poly[i - 1], poly[i], poly[(i + 1) % m]]))
        if area([poly[k - 1], poly[k], poly[(k + 1) % m]]) >= tol:
            break
        poly.pop(k)
    return poly


if mode == "islands":
    parts = [simplify([tuple(q) for q in i["hull"]]) for i in measure["visibleRockIslands"]]
    total = sum(i["m2"] for i in measure["visibleRockIslands"])
else:
    cx, cz = measure["centre"]
    radii = measure["footRadiusPer5Deg"]
    n = len(radii)
    pts = [(cx + r * math.cos(math.radians(5 * k)), cz - r * math.sin(math.radians(5 * k))) for k, r in enumerate(radii)]
    c = (cx, cz)
    start = min(range(n), key=lambda k: radii[k])
    parts, k, done = [], start, 0
    while done < n:
        chain = [pts[k % n], pts[(k + 1) % n]]
        j = 1
        while done + j < n and convex([c] + chain + [pts[(k + j + 1) % n]]):
            chain.append(pts[(k + j + 1) % n])
            j += 1
        parts.append([c] + chain)
        done += j
        k += j
    star = area(pts)
    assert abs(sum(area(p) for p in parts) - star) < 1e-3
    total = star
assert all(convex(p) for p in parts)
print(f"// {len(parts)} convex parts, {sum(area(p) for p in parts):.0f} m² (visible rock "
      f"{measure['rockM2']:.0f} m², {total:.0f} m² measured)")
for p in parts:
    print("    &[" + ", ".join(f"[{x:.2f}, {z:.2f}]" for x, z in p) + "],")
