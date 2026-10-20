"""Fern frond tiles for fern_clump (refs: assets-source/refs/forest, "ferns"), drawn in numpy only (no
bpy), so they run and can be inspected in any Python. Arrays are float32 in Blender pixel order (row 0
is the bottom, v = 0); u runs along the frond with its stipe at x = 0.

A frond is a once-pinnate blade: a bare stipe, a rachis, and alternating lanceolate pinnae with
scalloped (lobed) edges, angled toward the tip, under a lanceolate envelope (widest about a third of
the way up, pointed tip). The pinnae droop away from the rachis (the normal tilts outward across the
tile), each folds along its own midrib, and the fresh tips are lighter. The colour under every
transparent texel is dilated from the nearest pinna, so mips and bilinear taps stay green."""
from __future__ import annotations

import hashlib
import math

import numpy as np

import _props

ATLAS = 1024
TILES = 3
TILE_H = 336  # px across a tile; a frond card is ATLAS / TILE_H = 3.05 times longer than wide
ASPECT = ATLAS / TILE_H
DARK = np.array((0x23, 0x45, 0x18), np.float32) / 255
MID = np.array((0x3f, 0x72, 0x2a), np.float32) / 255
LIGHT = np.array((0x7a, 0xa8, 0x48), np.float32) / 255
STIPE = np.array((0x5a, 0x4a, 0x2a), np.float32) / 255
STIPE_END, TIP = 0.13, 0.985  # where the blade starts and ends, fraction of the tile length


def _rng(seed: str, k: int) -> np.random.Generator:
    return np.random.default_rng(int.from_bytes(hashlib.sha256(f"{seed}:frond:{k}".encode()).digest()[:8], "big"))


def envelope(s: np.ndarray | float) -> np.ndarray | float:
    """Pinna reach (fraction of the tile's half height) at s in [0, 1] along the blade: lanceolate,
    0.45 at the blade's foot, full at s = 0.32, pointed at the tip."""
    s = np.clip(s, 0.0, 1.0)
    rise = 0.45 + 0.55 * np.sin(np.minimum(s / 0.32, 1.0) * math.pi / 2)
    fall = np.clip((1.0 - s) / 0.68, 0.0, 1.0) ** 0.75
    return np.where(s < 0.32, rise, fall)


def frond_tile(w: int, h: int, seed: str, k: int):
    """One frond tile (w along, h across). Returns (albedo RGBA with sRGB RGB, normal RGB, roughness,
    ao), each (h, w, n) or (h, w)."""
    rng = _rng(seed, k)
    ys, xs = np.mgrid[0:h, 0:w].astype(np.float32) + 0.5
    cy = h / 2
    rgb = np.zeros((h, w, 3), np.float32)
    alpha = np.zeros((h, w), np.float32)
    nrm = np.empty((h, w, 3), np.float32)
    nrm[:] = (0.5, 0.5, 1.0)
    rough = np.full((h, w), 0.75, np.float32)
    margin = 4.0
    x0, x1 = STIPE_END * w, TIP * w
    blade = x1 - x0
    reach_px = cy - margin - 1.0

    def pinna(bx: float, by: float, side: int, length: float, ang: float, base_hw: float, tone: float, young: float):
        """One pinna from (bx, by) on the rachis, toward +x by `ang` and outward by `side`."""
        ux, uy = math.cos(ang), side * math.sin(ang)
        ex, ey = bx + ux * length, by + uy * length
        pad = base_hw + 2
        lo_x, hi_x = int(max(min(bx, ex) - pad, 0)), int(min(max(bx, ex) + pad + 1, w))
        lo_y, hi_y = int(max(min(by, ey) - pad, 0)), int(min(max(by, ey) + pad + 1, h))
        if lo_x >= hi_x or lo_y >= hi_y:
            return
        sub = (slice(lo_y, hi_y), slice(lo_x, hi_x))
        px, py = xs[sub] - bx, ys[sub] - by
        t = (px * ux + py * uy) / length
        bend = 0.08 * length * side  # tips curve gently toward the frond tip
        off = -px * uy + py * ux - bend * t * t * side
        lobes = max(3.0, length / (base_hw * 1.5))
        hw = base_hw * np.clip(1.0 - t, 0.0, 1.0) ** 0.55 * (0.74 + 0.26 * np.abs(np.cos(math.pi * t * lobes)))
        hw = np.where(t < 0.06, base_hw * (0.55 + 7.5 * np.clip(t, 0, None)), hw)  # narrow stalk at the rachis
        inside = (t >= 0.0) & (t <= 1.0) & (np.abs(off) < hw) & (alpha[sub] < 0.5)
        if not inside.any():
            return
        across = np.clip(off / np.maximum(hw, 1e-3), -1.0, 1.0)
        base = DARK + (MID - DARK) * tone
        tipc = MID + (LIGHT - MID) * young
        col = base[None, None] + (tipc - base)[None, None] * np.clip(t, 0, 1)[..., None] ** 0.8
        col = col * (1.0 - 0.16 * across ** 2)[..., None]
        rib = np.abs(across) < 0.14
        col = np.where(rib[..., None], col * 1.12 + 0.02, col)
        rgb[sub] = np.where(inside[..., None], np.clip(col, 0, 1), rgb[sub])
        alpha[sub] = np.where(inside, 1.0, alpha[sub])
        # Folded along the midrib, and drooping away from the rachis.
        fx, fy = -uy * across * 0.38, ux * across * 0.38
        droop = (ys[sub] - cy) / cy * 0.32
        nx, ny = fx, fy + droop
        nz = np.sqrt(np.clip(1 - nx * nx - ny * ny, 0.08, 1))
        nrm[sub] = np.where(inside[..., None], np.stack([nx, ny, nz], -1) * 0.5 + 0.5, nrm[sub])
        rough[sub] = np.where(inside, 0.62 + 0.1 * np.abs(across), rough[sub])

    # Pinnae, alternating sides, spacing shrinking toward the tip.
    n_pairs = int(rng.integers(19, 23))
    young = 0.55 + 0.25 * rng.random()
    s, side = 0.0, 1
    sites = []
    while s < 0.97:
        sites.append((s, side))
        step = (1.0 / n_pairs) * (1.15 - 0.5 * s) * 0.5 * rng.uniform(0.9, 1.1)
        s += step
        side = -side
    for s, side in sites:
        env = float(envelope(s))
        ang = math.radians(rng.uniform(56, 68) - 14 * s)
        length = env * reach_px / math.sin(ang)
        bx = x0 + s * blade
        spacing = blade * (1.0 / n_pairs) * (1.15 - 0.5 * s)
        base_hw = max(2.2, 0.44 * spacing * (0.6 + 0.4 * env))
        tone = float(rng.uniform(0.35, 1.0))
        pinna(bx, cy, side, max(length, 4.0), ang, base_hw, tone, min(1.0, young * (0.6 + 0.6 * s)))
    # Rachis and stipe over the pinna roots.
    t = np.clip(xs / w, 0, 1)
    half = np.where(xs < x0, 3.2, 3.0 - 1.9 * np.clip((xs - x0) / blade, 0, 1))
    on = (np.abs(ys - cy) < half) & (xs > margin) & (xs < x1 + 2)
    stalk = np.where((xs < x0)[..., None], STIPE[None, None] * (0.85 + 0.3 * t[..., None]),
                     MID[None, None] * 0.82 + LIGHT[None, None] * 0.18)
    rgb = np.where(on[..., None], stalk, rgb)
    alpha = np.where(on, 1.0, alpha)
    across = np.clip((ys - cy) / np.maximum(half, 1e-3), -1, 1)
    rn = np.stack([np.zeros_like(across), across * 0.5, np.sqrt(1 - 0.25 * across ** 2)], -1) * 0.5 + 0.5
    nrm = np.where(on[..., None], rn, nrm)
    rough = np.where(on, 0.7, rough)
    alpha[:, : int(margin)] = 0.0
    alpha[:, w - int(margin):] = 0.0
    alpha[: int(margin)] = 0.0
    alpha[h - int(margin):] = 0.0
    rgb = _props.dilate(rgb, alpha, iters=40)
    rgb[alpha <= 0.5] = np.where(rgb[alpha <= 0.5].sum(-1, keepdims=True) > 0, rgb[alpha <= 0.5], MID)
    # AO: the clump's heart is shaded (the stipes and the blade's foot), the outer blade open.
    ao = (0.55 + 0.45 * np.clip((xs / w - 0.05) / 0.4, 0, 1) ** 0.8).astype(np.float32)
    ao *= 1.0 - 0.12 * np.clip(1.0 - np.abs(ys - cy) / (0.35 * cy), 0, 1)  # shade along the rachis
    return np.concatenate([rgb, alpha[..., None]], -1), nrm, rough, ao


def frond_atlas(seed: str):
    """The atlas: TILES frond tiles stacked in v (tile 0 lowest), the rows above them spare (opaque
    MID, alpha 0). Returns (albedo RGBA, normal RGB, roughness, ao) of side ATLAS."""
    alb = np.zeros((ATLAS, ATLAS, 4), np.float32)
    alb[..., :3] = MID
    nrm = np.zeros((ATLAS, ATLAS, 3), np.float32)
    nrm[:] = (0.5, 0.5, 1.0)
    rough = np.full((ATLAS, ATLAS), 0.75, np.float32)
    ao = np.ones((ATLAS, ATLAS), np.float32)
    for k in range(TILES):
        a, n, r, o = frond_tile(ATLAS, TILE_H, seed, k)
        sl = slice(k * TILE_H, (k + 1) * TILE_H)
        alb[sl], nrm[sl], rough[sl], ao[sl] = a, n, r, o
    return alb, nrm, rough, ao


def tile_coverage(alb: np.ndarray) -> dict[str, list[float]]:
    """Alpha > 0.5 coverage per box-filtered mip, per tile and over the tile rows."""
    out = {f"frond_{k}": _props.coverage_by_mip(alb[k * TILE_H:(k + 1) * TILE_H, :, 3]) for k in range(TILES)}
    out["tiles"] = _props.coverage_by_mip(alb[: TILES * TILE_H, :, 3])
    return out
