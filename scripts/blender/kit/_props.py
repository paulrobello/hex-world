"""Meadow-prop card tiles for wildflower_clump (PRD §5.4, §6, §9.1 foliage), drawn in numpy only
(no bpy), so they run and can be inspected in any Python. Arrays are float32 in Blender pixel order
(row 0 is the bottom, v = 0)."""
from __future__ import annotations

import hashlib
import math
import random

import numpy as np

# Flower species from the owner-approved references (assets-source/refs/meadow_props): white
# daisies, yellow buttercups, purple clover, blue cornflowers. sRGB.
GRASS_DARK, GRASS_LIGHT = np.array([0x3a, 0x62, 0x1c]) / 255, np.array([0x9c, 0xc4, 0x48]) / 255
SPECIES = ("daisy", "buttercup", "clover", "cornflower")


def unit_hash(*key) -> float:
    """Deterministic value in [0, 1) from a key (never the built-in hash(), which is salted)."""
    h = hashlib.sha256(":".join(map(str, key)).encode()).digest()
    return int.from_bytes(h[:8], "little") / 2.0 ** 64


def coverage_by_mip(alpha: np.ndarray, cutoff: float = 0.5) -> list[float]:
    """Fraction of texels above `cutoff` at every box-filtered mip of a 2D alpha array."""
    cov, m = [], alpha.astype(np.float32)
    while True:
        cov.append(round(float((m > cutoff).mean()), 4))
        if min(m.shape) < 2:
            return cov
        h2, w2 = m.shape[0] // 2 * 2, m.shape[1] // 2 * 2
        m = m[:h2, :w2].reshape(h2 // 2, 2, w2 // 2, 2).mean(axis=(1, 3))


def dilate(rgb: np.ndarray, alpha: np.ndarray, iters: int = 24) -> np.ndarray:
    """Spread opaque colour into transparent texels (4-neighbour average), so mips and bilinear
    filtering at the alpha edge never pull in a foreign colour."""
    rgb = rgb.copy()
    known = alpha > 0.5
    for _ in range(iters):
        if known.all():
            break
        acc = np.zeros_like(rgb)
        cnt = np.zeros(known.shape, np.float32)
        for dy, dx in ((1, 0), (-1, 0), (0, 1), (0, -1)):
            k = np.roll(known, (dy, dx), (0, 1))
            acc += np.roll(rgb, (dy, dx), (0, 1)) * k[..., None]
            cnt += k
        grow = ~known & (cnt > 0)
        rgb[grow] = acc[grow] / cnt[grow][:, None]
        known = known | grow
    return rgb


def _blade(rgb, alpha, nrm, x, y, x0, y0, x1, y1, width, colour, rng):
    """One tapered, slightly curved grass blade from (x0, y0) (base) to (x1, y1) (tip), tile units."""
    dx, dy = x1 - x0, y1 - y0
    length = math.hypot(dx, dy)
    ux, uy = dx / length, dy / length
    bend = rng.uniform(-0.12, 0.12) * length
    px, py = x - x0, y - y0
    t = (px * ux + py * uy) / length  # 0 base .. 1 tip
    off = -px * uy + py * ux - bend * t * t  # signed distance across the (bent) blade axis
    half = width * 0.5 * np.clip(1.0 - t, 0.0, 1.0) ** 0.8
    inside = (t >= 0) & (t <= 1) & (np.abs(off) < half)
    if not inside.any():
        return
    across = np.clip(off / np.maximum(half, 1e-6), -1, 1)
    shade = (0.75 + 0.35 * t) * (1.0 - 0.18 * across ** 2)
    col = colour[None, None, :] * shade[..., None]
    rgb[:] = np.where(inside[..., None], np.clip(col, 0, 1), rgb)
    alpha[:] = np.where(inside, 1.0, alpha)
    # A folded blade: the normal leans across it, away from its midrib.
    nx = -uy * across * 0.45
    ny = ux * across * 0.45
    nz = np.sqrt(np.clip(1 - nx * nx - ny * ny, 0.05, 1))
    nrm[:] = np.where(inside[..., None], np.stack([nx, ny, nz], -1) * 0.5 + 0.5, nrm)


def _disc(rgb, alpha, nrm, x, y, cx, cy, r, colour, dome=0.6):
    d2 = ((x - cx) ** 2 + (y - cy) ** 2) / (r * r)
    inside = d2 < 1.0
    if not inside.any():
        return inside
    rgb[:] = np.where(inside[..., None], colour[None, None, :] * (1.0 - 0.15 * d2)[..., None], rgb)
    alpha[:] = np.where(inside, 1.0, alpha)
    nx, ny = (x - cx) / r * dome, (y - cy) / r * dome
    nz = np.sqrt(np.clip(1 - nx * nx - ny * ny, 0.05, 1))
    nrm[:] = np.where(inside[..., None], np.stack([nx, ny, nz], -1) * 0.5 + 0.5, nrm)
    return inside


def _petals(rgb, alpha, nrm, x, y, cx, cy, r, n, colour, rng, width=0.42, inner=0.0):
    """A radial flower head of n petals (side or top view squashed by the caller's y scale)."""
    ang = np.arctan2(y - cy, x - cx)
    rad = np.hypot(x - cx, y - cy) / r
    phase = rng.uniform(0, 2 * math.pi)
    lobe = np.abs(np.cos((ang - phase) * n / 2)) ** (1.0 / max(width, 1e-3))
    inside = (rad < 0.35 + 0.65 * lobe) & (rad >= inner)
    if not inside.any():
        return
    col = colour[None, None, :] * (0.82 + 0.18 * np.clip(1 - rad, 0, 1))[..., None]
    rgb[:] = np.where(inside[..., None], col, rgb)
    alpha[:] = np.where(inside, 1.0, alpha)
    nx, ny = (x - cx) / r * 0.35, (y - cy) / r * 0.35
    nz = np.sqrt(np.clip(1 - nx * nx - ny * ny, 0.05, 1))
    nrm[:] = np.where(inside[..., None], np.stack([nx, ny, nz], -1) * 0.5 + 0.5, nrm)


def flower_head(rgb, alpha, nrm, x, y, cx, cy, r, species, rng):
    """One head of `species` centred at (cx, cy), radius r in tile units."""
    if species == "daisy":
        _petals(rgb, alpha, nrm, x, y, cx, cy, r, 16, np.array([0.95, 0.94, 0.90]), rng, width=0.35)
        _disc(rgb, alpha, nrm, x, y, cx, cy, r * 0.32, np.array([0.93, 0.72, 0.12]))
    elif species == "buttercup":
        _petals(rgb, alpha, nrm, x, y, cx, cy, r, 5, np.array([0.98, 0.82, 0.10]), rng, width=0.9)
        _disc(rgb, alpha, nrm, x, y, cx, cy, r * 0.25, np.array([0.80, 0.62, 0.08]))
    elif species == "clover":
        # A rounded pom-pom of florets: many small discs, darker at the base.
        for k in range(14):
            a, d = rng.uniform(0, 2 * math.pi), r * 0.7 * math.sqrt(rng.random())
            c = np.array([0.62, 0.32, 0.58]) * rng.uniform(0.85, 1.12)
            _disc(rgb, alpha, nrm, x, y, cx + d * math.cos(a), cy + d * math.sin(a), r * 0.38,
                  np.clip(c, 0, 1))
    else:  # cornflower: ragged blue florets around a dark-violet centre
        _petals(rgb, alpha, nrm, x, y, cx, cy, r, 9, np.array([0.30, 0.45, 0.90]), rng, width=0.6)
        _petals(rgb, alpha, nrm, x, y, cx, cy, r * 0.7, 7, np.array([0.38, 0.52, 0.95]), rng, width=0.5)
        _disc(rgb, alpha, nrm, x, y, cx, cy, r * 0.22, np.array([0.25, 0.18, 0.45]))


def tuft_tile(px: int, species: str | None, seed: str, k: int):
    """A side-view tuft card: dense grass blades rooted along the bottom, a solid sod band at the
    very bottom (sunk below ground at runtime) and, for a species, flower heads on stems above it.
    Alpha falls to 0 at least MARGIN inside the left, right and top edges (no visible card edge).
    Returns (albedo RGBA with sRGB RGB, normal RGB, roughness)."""
    rng = random.Random(f"{seed}:tuft:{species}:{k}")
    ys, xs = np.mgrid[0:px, 0:px].astype(np.float32)
    x, y = (xs + 0.5) / px, (ys + 0.5) / px
    rgb = np.zeros((px, px, 3), np.float32)
    alpha = np.zeros((px, px), np.float32)
    nrm = np.empty((px, px, 3), np.float32)
    nrm[:] = (0.5, 0.5, 1.0)
    margin = 0.06
    # Sod band: rounded mound so the bottom corners are clear too.
    sod = (y < 0.16 - 0.5 * (x - 0.5) ** 2 * 1.6) & (x > margin) & (x < 1 - margin)
    rgb[sod] = GRASS_DARK * 0.85
    alpha[sod] = 1.0
    n_blades = 170
    for i in range(n_blades):
        bx = 0.5 + rng.gauss(0, 0.17)
        bx = min(max(bx, margin + 0.04), 1 - margin - 0.04)
        h = rng.uniform(0.45, 0.9) * (1.0 - 1.1 * abs(bx - 0.5))
        lean = rng.uniform(-0.18, 0.18) + (bx - 0.5) * 0.5
        tx = min(max(bx + lean * h, margin), 1 - margin)
        ty = min(0.06 + h, 1 - margin)
        tone = rng.uniform(0.0, 1.0) * 0.7 + 0.3 * (i / n_blades)
        col = GRASS_DARK + (GRASS_LIGHT - GRASS_DARK) * tone
        _blade(rgb, alpha, nrm, x, y, bx, 0.02, tx, ty, rng.uniform(0.026, 0.044), col, rng)
    if species:
        heads = rng.randint(4, 6)
        for i in range(heads):
            bx = 0.5 + (i - (heads - 1) / 2) / heads * 0.62 + rng.uniform(-0.04, 0.04)
            hy = rng.uniform(0.5, 0.8)
            hx = bx + rng.uniform(-0.06, 0.06)
            r = rng.uniform(0.07, 0.095) * (1.15 if species == "clover" else 1.0)
            hx = min(max(hx, margin + r), 1 - margin - r)
            hy = min(hy, 1 - margin - r)
            _blade(rgb, alpha, nrm, x, y, bx, 0.03, hx, hy, 0.016, GRASS_DARK * 1.2, rng)
            flower_head(rgb, alpha, nrm, x, y, hx, hy, r, species, rng)
    rough = np.where(alpha > 0.5, 0.7, 0.8).astype(np.float32)
    rgb = dilate(rgb, alpha)
    rgb[alpha <= 0.5] = np.where(rgb[alpha <= 0.5].sum(-1, keepdims=True) > 0, rgb[alpha <= 0.5],
                                 GRASS_DARK)
    return np.concatenate([rgb, alpha[..., None]], -1), nrm, rough


def top_tile(px: int, seed: str, k: int):
    """A top-view card: a few heads of every species scattered over a grass-blade rosette, read
    from the strategic camera. Alpha is 0 outside a disc well inside the tile."""
    rng = random.Random(f"{seed}:top:{k}")
    ys, xs = np.mgrid[0:px, 0:px].astype(np.float32)
    x, y = (xs + 0.5) / px, (ys + 0.5) / px
    rgb = np.zeros((px, px, 3), np.float32)
    alpha = np.zeros((px, px), np.float32)
    nrm = np.empty((px, px, 3), np.float32)
    nrm[:] = (0.5, 0.5, 1.0)
    for i in range(110):  # rosette of blades radiating from the centre
        a = rng.uniform(0, 2 * math.pi)
        r0, r1 = rng.uniform(0.0, 0.1), rng.uniform(0.3, 0.44)
        tone = rng.random()
        col = GRASS_DARK + (GRASS_LIGHT - GRASS_DARK) * tone
        _blade(rgb, alpha, nrm, x, y, 0.5 + r0 * math.cos(a), 0.5 + r0 * math.sin(a),
               0.5 + r1 * math.cos(a), 0.5 + r1 * math.sin(a), rng.uniform(0.06, 0.09), col, rng)
    order = list(SPECIES) * 2
    rng.shuffle(order)
    for i, sp in enumerate(order):
        a = 2 * math.pi * (i + rng.random() * 0.6) / len(order)
        d = rng.uniform(0.1, 0.3)
        flower_head(rgb, alpha, nrm, x, y, 0.5 + d * math.cos(a), 0.5 + d * math.sin(a),
                    rng.uniform(0.065, 0.09), sp, rng)
    rough = np.where(alpha > 0.5, 0.7, 0.8).astype(np.float32)
    rgb = dilate(rgb, alpha)
    return np.concatenate([rgb, alpha[..., None]], -1), nrm, rough


GRID = 4
# (kind, species, variant) per (col, row) of the GRID x GRID atlas; row 0 is the bottom (v = 0).
LAYOUT = {(0, 0): ("tuft", None, 0), (1, 0): ("tuft", None, 1), (2, 0): ("tuft", None, 2), (3, 0): ("tuft", None, 3),
          (0, 1): ("tuft", "daisy", 0), (1, 1): ("tuft", "daisy", 1), (2, 1): ("tuft", "buttercup", 0),
          (3, 1): ("tuft", "buttercup", 1), (0, 2): ("tuft", "clover", 0), (1, 2): ("tuft", "clover", 1),
          (2, 2): ("tuft", "cornflower", 0), (3, 2): ("tuft", "cornflower", 1),
          (0, 3): ("top", None, 0), (1, 3): ("top", None, 1), (2, 3): ("top", None, 2), (3, 3): ("tuft", None, 4)}


def clump_atlas(px: int, seed: str):
    """The clump atlas: GRID x GRID tiles of px square (LAYOUT), in Blender pixel order. Returns
    (albedo RGBA, normal RGB, roughness, ao) arrays of side GRID * px. AO darkens tufts toward their
    roots (a unique ORM.R; masks.A stays 1)."""
    n = GRID * px
    alb = np.zeros((n, n, 4), np.float32)
    nrm = np.zeros((n, n, 3), np.float32)
    rough = np.zeros((n, n), np.float32)
    ao = np.ones((n, n), np.float32)
    v = (np.arange(px, dtype=np.float32) + 0.5) / px
    for (c, r), (kind, sp, k) in LAYOUT.items():
        a, nm, ro = top_tile(px, seed, k) if kind == "top" else tuft_tile(px, sp, seed, k)
        sl = (slice(r * px, (r + 1) * px), slice(c * px, (c + 1) * px))
        alb[sl], nrm[sl], rough[sl] = a, nm, ro
        if kind == "tuft":
            ao[sl] = (0.65 + 0.35 * np.clip(v / 0.5, 0, 1))[:, None]
    return alb, nrm, rough, ao
