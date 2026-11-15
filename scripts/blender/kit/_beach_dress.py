"""Dressing centres for the beach tiles (refs/beach ref-1): pebbles in small groups at the grass edge.
The dune grass and driftwood are scatter props since 2026-10-20 (owner D3, BD-Q1: replace the
baked logs and tufts, keep the pebbles; `beach_dunegrass*`, `beach_driftwood*`), so the logs and
tufts are still placed here, to keep every pebble where it was approved, but no longer built.
Colour-only slots plus the piece's §9.4 masks map on the planar UV0 (`masks_field`). Everything sits on dry land (s <= DRY_S) and at least BORDER_M inside the border,
so nothing changes the bed under the water (the water reads the prepass depth as its bed) and
nothing crosses a seam. Kept apart from `_beach.py` so the bases do not depend on it."""
import math
import random

import bmesh
import numpy as np

from lib import hexgeo

import _beach

TUFT, WOOD, PEBBLE = "beach_dunegrass", "beach_driftwood", "beach_pebble"
DRESS_SLOTS = (TUFT, WOOD, PEBBLE)
COLORS = {TUFT: ("#7d8b45", 0.85), WOOD: ("#a39a8a", 0.9), PEBBLE: ("#6f675d", 0.6)}
DRY_S = -2.4  # the damp band's top is at s = -1.95
BORDER_M = 1.5
MASK_SIZE = 256  # 8 px/m on the 32 m planar UV0: every channel is a smooth field of (x, y)
POROSITY = (0.55, 1.0)  # G at the grass edge (s = -10) and toward the damp band (s = DRY_S)


def masks_field(layout):
    """The §9.4 masks (R, G, B, A), each a (size*size,) array in Blender pixel order over the planar
    UV0 (`uv.planar_cap_uv`, the 2R square). An analytic field of (x, y), so no bake and no change to
    the GLB. R 1: snow lies on the logs, pebbles and tufts alike, and the shader keeps it to up-facing
    faces. G porosity, rising from POROSITY[0] at the grass edge to POROSITY[1] toward the damp band.
    B 0: nothing glows. A 1: these slots had no AO before the map, so the MVP look is unchanged."""
    c = (np.arange(MASK_SIZE, dtype=np.float64) + 0.5) / MASK_SIZE * 2 * hexgeo.HEX_R - hexgeo.HEX_R
    xs, ys = np.meshgrid(c, c)
    s = np.vectorize(lambda x, y: _beach.seaward(layout, x, y))(xs, ys)
    t = np.clip((s + 10.0) / (DRY_S + 10.0), 0.0, 1.0)
    wet = POROSITY[0] + (POROSITY[1] - POROSITY[0]) * t * t * (3.0 - 2.0 * t)
    n = MASK_SIZE * MASK_SIZE
    return (np.ones(n, np.float32), wet.ravel().astype(np.float32), np.zeros(n, np.float32),
            np.ones(n, np.float32))


def strip_point(layout, s, u):
    """Blender (x, y) at seaward distance s, fraction u along the coast between the beach edges."""
    ka, kb = _beach.LAYOUTS[layout]["beach"]
    pa, pb = _beach.edge_point(layout, ka, s), _beach.edge_point(layout, kb, s)
    if layout == "straight":
        return (pa[0] + (pb[0] - pa[0]) * u, pa[1] + (pb[1] - pa[1]) * u)
    cx = -_beach.ARC_C if layout == "outer" else _beach.ARC_C
    r = math.hypot(pa[0] - cx, pa[1])
    a0, a1 = math.atan2(pa[1], pa[0] - cx), math.atan2(pb[1], pb[0] - cx)
    if abs(a1 - a0) > math.pi:
        a1 += -2 * math.pi if a1 > a0 else 2 * math.pi
    a = a0 + (a1 - a0) * u
    return (cx + r * math.cos(a), r * math.sin(a))


def ground(layout, x, y):
    return _beach.height(_beach.seaward(layout, x, y))


def placements(piece, layout):
    """Deterministic [(kind, x, y, params)] seeded by the piece id."""
    rng = random.Random(piece)
    items = []

    def free(x, y, r):
        return hexgeo.dist_to_border(x, y) >= BORDER_M + r and all(
            math.hypot(x - i[1], y - i[2]) > r + i[3]["r"] + 0.15 for i in items)

    def place(kind, s_rng, u_rng, r, n, tries=60, **kw):
        for _ in range(n):
            for _t in range(tries):
                s, u = rng.uniform(*s_rng), rng.uniform(*u_rng)
                x, y = strip_point(layout, s, u)
                if _beach.seaward(layout, x, y) <= DRY_S and free(x, y, r):
                    items.append((kind, x, y, {"r": r, "seed": rng.randrange(1 << 30), **kw}))
                    break

    # Driftwood first (largest): logs lie along the coast on the dry sand.
    for _ in range(3):
        length = rng.uniform(1.6, 2.8)
        place(WOOD, (-6.0, -2.4), (0.12, 0.88), length / 2, 1, length=length, radius=rng.uniform(0.09, 0.15),
              yaw=rng.uniform(-25, 25))
    # Dune-grass clumps in loose groups straddling the grass edge (s = -8).
    for g in range(4):
        u0 = 0.1 + 0.8 * (g + rng.uniform(0.15, 0.85)) / 4
        for _ in range(rng.randint(2, 4)):
            place(TUFT, (-9.8, -5.8), (u0 - 0.06, u0 + 0.06), 0.4, 1)
    # Pebbles at the grass edge, in small groups.
    for g in range(5):
        u0 = rng.uniform(0.1, 0.9)
        for _ in range(rng.randint(3, 5)):
            place(PEBBLE, (-8.6, -6.2), (u0 - 0.04, u0 + 0.04), rng.uniform(0.08, 0.18), 1)
    return items


def _pebble(bm, layout, x, y, p):
    rnd = random.Random(p["seed"])
    m = bmesh.ops.create_icosphere(bm, subdivisions=1, radius=1.0)
    w, h = p["r"], p["r"] * rnd.uniform(0.45, 0.65)
    z = ground(layout, x, y) + 0.2 * h
    for v in m["verts"]:
        j = 1.0 + 0.2 * (rnd.random() - 0.5)
        v.co.x = x + v.co.x * w * j
        v.co.y = y + v.co.y * w * 0.85 * j
        v.co.z = z + v.co.z * h * j
