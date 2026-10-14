"""tree_hero_oak leaf-cluster tiles, drawn procedurally (numpy only, no bpy, so it is testable from any
Python): LEAF_TILES variants, each a layered cluster of lobed oak leaves (ref-1). Shared by the live
authoring session (the leaf material's image textures) and the export stage (the atlas leaf strip),
so the showcase and the engine show the same leaves.

Arrays are float32 in Blender pixel order (row 0 is the bottom, v = 0): albedo RGBA with sRGB-encoded
RGB and the alpha mask in A, a tangent-space normal (OpenGL, +Y = +v) in RGB, roughness in R."""
from __future__ import annotations

import math
import random

import numpy as np

TILES = 4
LEAVES = 220  # per tile, drawn back to front
LOBES = 3.5  # rounded lobes per leaf side, oak style
CLUSTER_R = 0.45  # leaf centres fall inside this disc (tile units)
DARK, LIGHT = np.array([0x2f, 0x4d, 0x1c]) / 255, np.array([0x7d, 0x9e, 0x3a]) / 255


def _srgb_mix(a: np.ndarray, b: np.ndarray, t: float) -> np.ndarray:
    return a + (b - a) * t


def tile(px: int, k: int, seed: str) -> tuple[np.ndarray, np.ndarray, np.ndarray]:
    """One cluster tile: (albedo RGBA, normal RGB, roughness) as (px, px, n) float32 arrays."""
    rng = random.Random(f"{seed}:leaftile:{k}")
    ys, xs = np.mgrid[0:px, 0:px].astype(np.float32)
    x, y = (xs + 0.5) / px, (ys + 0.5) / px
    rgb = np.empty((px, px, 3), np.float32)
    rgb[:] = _srgb_mix(DARK, LIGHT, 0.45)  # transparent texels carry leaf green so mips stay green
    alpha = np.zeros((px, px), np.float32)
    nrm = np.empty((px, px, 3), np.float32)
    nrm[:] = (0.5, 0.5, 1.0)
    rough = np.full((px, px), 0.6, np.float32)
    for i in range(LEAVES):
        layer = i / (LEAVES - 1)  # later leaves sit on top: brighter, nearer the dome surface
        ang, rad = rng.uniform(0, 2 * math.pi), CLUSTER_R * math.sqrt(rng.random()) * (1.0 - 0.25 * layer)
        cx, cy = 0.5 + rad * math.cos(ang), 0.5 + rad * math.sin(ang)
        length = rng.uniform(0.12, 0.17) * (0.9 + 0.2 * layer)
        half_w = length * rng.uniform(0.3, 0.36)
        # Leaves point outward from the cluster centre, with spread, like ref-1's rosettes.
        theta = math.atan2(cy - 0.5, cx - 0.5) + rng.uniform(-0.9, 0.9)
        c, s = math.cos(theta), math.sin(theta)
        ext = length * 0.62
        x0, x1 = max(int((cx - ext) * px), 0), min(int((cx + ext) * px) + 1, px)
        y0, y1 = max(int((cy - ext) * px), 0), min(int((cy + ext) * px) + 1, px)
        if x0 >= x1 or y0 >= y1:
            continue
        dx, dy = x[y0:y1, x0:x1] - cx, y[y0:y1, x0:x1] - cy
        sl = (dx * c + dy * s) / (length / 2)  # -1 stem .. +1 tip, along the midrib
        q = -dx * s + dy * c  # across the midrib
        env = np.sqrt(np.clip(1 - sl * sl, 0, 1)) ** 0.9 * (0.78 + 0.22 * sl)  # obovate outline
        phase = rng.uniform(0, 0.4)
        lobes = 0.6 + 0.4 * (0.5 + 0.5 * np.cos(2 * math.pi * LOBES * (sl + 1) / 2 + phase)) ** 0.7
        half = half_w * env * lobes
        blade = (np.abs(sl) <= 1) & (np.abs(q) < half)
        stalk = (sl > -1.18) & (sl < -0.9) & (np.abs(q) < 0.0035)
        inside = blade | stalk
        if not inside.any():
            continue
        tone = rng.uniform(0.15, 0.85) * 0.6 + 0.4 * layer
        base = _srgb_mix(DARK, LIGHT, tone) * (0.82 + 0.25 * cy)
        across = np.clip(np.abs(q) / np.maximum(half, 1e-6), 0, 1)
        shade = (1.0 - 0.28 * across ** 2) * (0.88 + 0.12 * (sl + 1) / 2)
        col = base[None, None, :] * shade[..., None]
        midrib = np.abs(q) < 0.0028
        col = np.where(midrib[..., None], col * 1.18, col)
        # Fold along the midrib plus a random tilt: the normal leans across the leaf, away from the rib.
        fold = np.sign(q) * across * 0.5
        tilt_a, tilt_b = rng.uniform(-0.25, 0.25), rng.uniform(-0.25, 0.25)
        nx = -s * fold + c * tilt_a - s * tilt_b
        ny = c * fold + s * tilt_a + c * tilt_b
        nz = np.sqrt(np.clip(1 - nx * nx - ny * ny, 0.05, 1))
        sub = (slice(y0, y1), slice(x0, x1))
        rgb[sub] = np.where(inside[..., None], np.clip(col, 0, 1), rgb[sub])
        alpha[sub] = np.where(inside, 1.0, alpha[sub])
        n3 = np.stack([nx, ny, nz], -1) * 0.5 + 0.5
        nrm[sub] = np.where(inside[..., None], n3, nrm[sub])
        rough[sub] = np.where(inside, rng.uniform(0.5, 0.66), rough[sub])
    albedo = np.concatenate([rgb, alpha[..., None]], -1)
    return albedo, nrm, rough


def outline(n: int = 26, phase: float = 0.2) -> list[tuple[float, float]]:
    """The same lobed, obovate oak-leaf outline as `tile`, as a closed polygon in leaf units: s runs
    -1 (stalk) to +1 (tip) along the midrib, q across it (half-width 0.33 at the widest). For the
    leaf geometry of the high-poly canopy domes (the bake source), so cards and domes share a leaf."""
    half_w = 0.33
    side = []
    for i in range(n + 1):
        sl = -1 + 2 * i / n
        env = math.sqrt(max(1 - sl * sl, 0.0)) ** 0.9 * (0.78 + 0.22 * sl)
        lob = 0.6 + 0.4 * (0.5 + 0.5 * math.cos(2 * math.pi * LOBES * (sl + 1) / 2 + phase)) ** 0.7
        side.append((sl, half_w * env * lob))
    return [(s, q) for s, q in side] + [(s, -q) for s, q in reversed(side[1:-1])]


def strip(px: int, seed: str) -> tuple[np.ndarray, np.ndarray, np.ndarray]:
    """TILES tiles side by side: arrays of shape (px, px * TILES, n)."""
    tiles = [tile(px, k, seed) for k in range(TILES)]
    return tuple(np.concatenate([t[i] for t in tiles], axis=1) for i in range(3))


if __name__ == "__main__":
    import sys

    a, n, r = strip(int(sys.argv[1]) if len(sys.argv) > 1 else 256, "tree_hero_oak")
    print("opaque", float((a[..., 3] > 0.5).mean()))
