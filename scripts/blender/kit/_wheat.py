"""Ripe-wheat row-card tiles and the field's earth profile for wheat_field (refs: assets-source/refs/farm,
"wheat crop"), drawn in numpy only (no bpy), so they run and can be inspected in any Python. Arrays are
float32 in Blender pixel order (row 0 is the bottom, v = 0).

A tile is one row card seen from the side: a dense, opaque body of golden stalks from the bottom (sunk
below the ridge at runtime) up to the ear line, and above it the ears with their awns, fading to alpha 0
before the top edge. The body holds the alpha coverage down the mip chain (`_props.coverage_by_mip`), so
a distant row stays a solid golden band under alphaCutoff 0.5. Tiles wrap horizontally (every stalk is
drawn with wrapped columns), so cards butted along a row join without a seam, whichever variant each
draws."""
from __future__ import annotations

import math
import random

import numpy as np

import _props

# sRGB tones of ripe wheat (the farm refs' straw, as `_hay.py` round bales: #b98f45 .. #d3b064).
STALK = np.array([0xb8, 0x90, 0x42]) / 255
STALK_DARK = np.array([0x6e, 0x52, 0x1f]) / 255
EAR = np.array([0xd6, 0xae, 0x55]) / 255
EAR_LIGHT = np.array([0xec, 0xd2, 0x8a]) / 255
AWN = np.array([0xe4, 0xcb, 0x86]) / 255


def undulation(x, y):
    """meadow_base's swell (`undulation()` in scripts/blender/kit/meadow_base.py), copied: that file is
    a generator, not an importable helper. Scalars only."""
    s = (0.6 * math.sin(0.31 * x + 1.7) * math.cos(0.27 * y - 0.4)
         + 0.4 * math.sin(0.53 * x - 0.45 * y + 0.9))
    return 0.5 + 0.5 * s


def meadow_relief(x, y):
    """meadow_base's cap relief more than 2 m inside the border (its falloff is 1 there): the field
    sits wholly inside that band, so the falloff is left out."""
    return -0.25 * undulation(x, y)


def _wrap_dx(dx, w):
    """dx between a wrapped column and an unwrapped centre, brought into [-w/2, w/2): per-pixel
    geometry (normal tilt) on a horizontally wrapping tile must not see the wrap's w-pixel jump."""
    return (dx + w / 2) % w - w / 2


def _stamp(img, mask_fn, x0, x1, y0, y1, w):
    """Calls mask_fn(xs, ys) on the window [x0, x1) x [y0, y1) of a w-wide wrapping image (x in pixel
    units, wrapped) and returns (rows, cols, mask) for the caller to write."""
    y0, y1 = max(int(math.floor(y0)), 0), min(int(math.ceil(y1)), img.shape[0])
    if y1 <= y0:
        return None
    xi = np.arange(int(math.floor(x0)), int(math.ceil(x1)))
    ys = np.arange(y0, y1)
    X, Y = np.meshgrid(xi + 0.5, ys + 0.5)
    m = mask_fn(X, Y)
    return ys[:, None], (xi % w)[None, :], m


def row_tile(w: int, h: int, card_w_m: float, card_h_m: float, sink_m: float, seed: str, k: int):
    """One row-card tile of w x h px covering card_w_m x card_h_m metres, its bottom sink_m below the
    ridge. Returns (albedo RGBA with sRGB RGB, normal RGB, roughness, ao), each (h, w[, c])."""
    rng = random.Random(f"{seed}:row:{k}")
    ppm_x, ppm_y = w / card_w_m, h / card_h_m
    ground = sink_m * ppm_y  # the ridge line, in px from the bottom
    rgb = np.zeros((h, w, 3), np.float32)
    alpha = np.zeros((h, w), np.float32)
    nrm = np.empty((h, w, 3), np.float32)
    nrm[:] = (0.5, 0.5, 1.0)
    depth = np.full((h, w), -1.0, np.float32)  # painter's order: nearer stalks overwrite farther ones

    def put(sl, m, col, nz_tilt, d):
        rows, cols, mask = sl
        cur = depth[rows, cols]
        win = mask & (d >= cur)
        if not win.any():
            return
        r_ = np.broadcast_to(rows, mask.shape)[win]
        c_ = np.broadcast_to(cols, mask.shape)[win]
        rgb[r_, c_] = col[win] if col.ndim == 3 else col
        alpha[r_, c_] = 1.0
        depth[r_, c_] = d
        nx = nz_tilt[win] if np.ndim(nz_tilt) else np.full(win.sum(), nz_tilt)
        nz = np.sqrt(np.clip(1 - nx * nx, 0.05, 1))
        nrm[r_, c_] = np.stack([nx * 0.5 + 0.5, np.full_like(nx, 0.5), nz * 0.5 + 0.5], -1)

    # Ripe wheat stands about 1 m tall and evenly topped: ear heads between 0.78 and 0.95 m above the
    # ridge, so the row reads as one level golden band at play distance.
    stalks = int(card_w_m * 300)
    for i in range(stalks):
        d = rng.random()  # depth: 0 back, 1 front
        x0 = rng.uniform(0, w)
        top_m = rng.uniform(0.78, 0.95)
        lean = rng.gauss(0, 0.05)  # m of tip drift per m of height
        nod = rng.uniform(0.02, 0.07)  # the heavy ear droops a little to one side
        if rng.random() < 0.5:
            nod = -nod
        ear_len = rng.uniform(0.085, 0.11)
        ear_w = rng.uniform(0.014, 0.019)
        stem_w = rng.uniform(0.0035, 0.0055)
        tone = rng.uniform(-1, 1)
        shade = 0.72 + 0.28 * d  # back stalks are darker (in the row's shade)
        y_top = ground + top_m * ppm_y
        # The stem: from the bottom edge up to the ear's base.
        y_ear0 = y_top - ear_len * ppm_y
        x_at = lambda Y, x0=x0, lean=lean: x0 + lean * ppm_x * np.clip(Y - ground, 0, None) / ppm_y  # noqa: E731
        half = max(stem_w * ppm_x * 0.5, 0.55)
        xe = x0 + lean * (y_ear0 - ground) / ppm_y * ppm_x

        def stem(X, Y, x_at=x_at, half=half):
            return np.abs(X - x_at(Y)) < half

        sl = _stamp(rgb, stem, min(x0, xe) - 3, max(x0, xe) + 3, 0, y_ear0, w)
        if sl:
            Y = sl[0] + 0.5
            f = np.clip((Y - ground) / max(y_ear0 - ground, 1), 0, 1)
            col = STALK_DARK + (STALK * (1 + 0.08 * tone) - STALK_DARK) * (0.35 + 0.65 * f)[..., None]
            col = np.broadcast_to(col * shade, sl[2].shape + (3,))
            put(sl, None, np.clip(col, 0, 1), np.clip(_wrap_dx(sl[1] + 0.5 - x_at(Y), w) / half, -1, 1) * 0.4, d)
        # The ear: a spindle tilted by `nod`, with grain rows across it.
        ax = xe + nod * ppm_x * 0.3

        def ear(X, Y, xe=xe, ax=ax, y_ear0=y_ear0, y_top=y_top, ear_w=ear_w):
            t = (Y - y_ear0) / max(y_top - y_ear0, 1)
            cx = xe + (ax - xe) * t
            r = ear_w * ppm_x * 0.5 * np.sqrt(np.clip(np.sin(np.pi * np.clip(t, 0, 1)), 0, 1)) + 0.4
            return (t >= 0) & (t <= 1) & (np.abs(X - cx) < r)

        sl = _stamp(rgb, ear, min(xe, ax) - 8, max(xe, ax) + 8, y_ear0, y_top + 1, w)
        if sl:
            rows, cols, m = sl
            Y = rows + 0.5
            X = np.broadcast_to(sl[1], m.shape).astype(np.float32)
            t = np.clip((Y - y_ear0) / max(y_top - y_ear0, 1), 0, 1)
            grain = 0.5 + 0.5 * np.sin((Y - y_ear0) / (0.009 * ppm_y) * 2 * np.pi)
            col = EAR + (EAR_LIGHT - EAR) * (0.25 + 0.5 * grain + 0.25 * tone)[..., None]
            col = np.broadcast_to(np.clip(col * (0.8 + 0.2 * d), 0, 1), m.shape + (3,))
            cx = xe + (ax - xe) * t
            put(sl, None, col, np.clip(_wrap_dx(X + 0.5 - cx, w) / (ear_w * ppm_x * 0.5 + 0.4), -1, 1) * 0.55, d + 0.001)
        # Awns: a few thin bristles above the ear (one pixel wide).
        for a in range(3):
            adx = rng.uniform(-0.02, 0.02) + nod * 0.3
            alen = rng.uniform(0.035, 0.06)
            bx = ax + rng.uniform(-0.3, 0.3) * ear_w * ppm_x
            by = y_top - rng.uniform(0.0, 0.25) * ear_len * ppm_y

            def awn(X, Y, bx=bx, by=by, adx=adx, alen=alen):
                t = (Y - by) / (alen * ppm_y)
                return (t >= 0) & (t <= 1) & (np.abs(X - (bx + adx * ppm_x * t)) < 0.5)

            sl = _stamp(rgb, awn, bx - abs(adx) * ppm_x - 2, bx + abs(adx) * ppm_x + 2, by,
                        by + alen * ppm_y + 1, w)
            if sl:
                put(sl, None, np.broadcast_to(AWN * (0.85 + 0.15 * d), sl[2].shape + (3,)), 0.0, d)
    # The body: below the ear line the row is solid (thin gaps between stalks would vanish down the
    # mips anyway); fill the gaps with the row's shaded depth so the card never shows the earth
    # behind. Above it, a ragged band of the ears' own tone (the ears of the stalks behind the card)
    # keeps the ear line golden and solid from above, where the card leans toward the camera.
    body = ground + 0.62 * ppm_y
    yy = (np.arange(h, dtype=np.float32) + 0.5)[:, None]
    xx = (np.arange(w, dtype=np.float32) + 0.5)[None, :] / w
    ragged = sum(a * np.sin(2 * np.pi * (f_ * xx + rng.random())) for a, f_ in ((0.03, 7), (0.02, 19), (0.012, 53)))
    crown = ground + (0.86 + ragged) * ppm_y
    fill = (alpha < 0.5) & (yy < body)
    f = np.clip((yy - ground) / (body - ground), 0, 1)
    under = STALK_DARK * 0.9 + (STALK * 0.7 - STALK_DARK * 0.9) * f[..., None]
    rgb = np.where(fill[..., None], np.broadcast_to(under, rgb.shape), rgb)
    alpha = np.where(fill, 1.0, alpha)
    band = (alpha < 0.5) & (yy >= body) & (yy < crown)
    t = np.clip((yy - body) / np.maximum(crown - body, 1), 0, 1)
    band_col = STALK * 0.7 + (EAR * 0.92 - STALK * 0.7) * np.broadcast_to(t, (h, w))[..., None]
    rgb = np.where(band[..., None], band_col, rgb)
    alpha = np.where(band, 1.0, alpha)
    rgb = _props.dilate(rgb, alpha)
    # Alpha 0 at least 4 px below the top edge, so no texel of the next tile up bleeds in.
    alpha[h - 4:] = 0.0
    rough = np.where(alpha > 0.5, 0.72, 0.8).astype(np.float32)
    # AO (ORM.R): dark at the ridge, open by the ears.
    ao = np.broadcast_to((0.45 + 0.55 * np.clip((yy - ground) / (0.8 * ppm_y), 0, 1) ** 0.8), (h, w))
    return np.concatenate([np.clip(rgb, 0, 1), alpha[..., None]], -1), nrm, rough, ao.astype(np.float32)



def atlas(size: int, rows: int, card_w_m: float, card_h_m: float, sink_m: float, seed: str):
    """`rows` row-card variants of size x (size // rows) px stacked from the bottom of a size x size
    atlas (any rows left over at the top repeat the last tile's colour at alpha 0). Returns (albedo
    RGBA, normal RGB, roughness, ao, tiles) with tiles = {name: (v0, v1)} in atlas V."""
    row_h = size // rows
    alb = np.zeros((size, size, 4), np.float32)
    nrm = np.zeros((size, size, 3), np.float32)
    nrm[:] = (0.5, 0.5, 1.0)
    rough = np.full((size, size), 0.8, np.float32)
    ao = np.ones((size, size), np.float32)
    tiles = {}
    for k in range(rows):
        a, n, r, o = row_tile(size, row_h, card_w_m, card_h_m, sink_m, seed, k)
        sl = slice(k * row_h, (k + 1) * row_h)
        alb[sl], nrm[sl], rough[sl], ao[sl] = a, n, r, o
        tiles[f"row{k}"] = (k * row_h / size, (k + 1) * row_h / size)
    if rows * row_h < size:
        alb[rows * row_h:, :, :3] = alb[rows * row_h - 8, :, :3]
    return alb, nrm, rough, ao, tiles
