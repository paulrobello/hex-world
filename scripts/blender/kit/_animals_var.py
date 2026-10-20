"""Farm animal variants (PRD §6 Variety) on the approved `_animals` builder: animal_cow_b (the cow
grazing, head down to the grass) and animal_sheep_b (a lamb). Refs: assets-source/refs/farm_animals
(owner A17: hornless brown-and-white cows, black-faced sheep, ref-2's smooth form).

`_animals.py` and `_fauna.py` are reused unchanged: a variant's spec is the original's station table
moved through per-part maps (`Map`: p' = M (p - src) + dst in the creature frame, with the fixed
frame vectors carried along), and a bent neck is a new loft along a cubic curve (`curve_part`). The
maps are drawn through the original's own paint function at each sample's rest position (`RestPaint`),
so a posed cow keeps the same patches, blaze, eyes and muzzle, and a lamb the sheep's face, hood and
wool line. The original generators are monolithic scripts, so each variant generator carries a copy
of its original's station table, palette and paint.

Every LOD is rebuilt from the moved spec with `_animals.build_lod` (never decimated), the open ring
ends are checked with `_animals.check_buried`, and the motion contract (TEXCOORD_1) is written and
re-read from the GLB by `_animals.check_motion` for the variant's own geometry."""
from __future__ import annotations

import math

import numpy as np

import _animals as an
import _fauna as fa

# What the variants rely on in the approved builder.
assert an.ATLAS == 1024 and an.SMALL == 512 and an.MASK_SIZE == 128, (an.ATLAS, an.SMALL, an.MASK_SIZE)
assert an.PART == {"body": 0, "head": 1, "leg_fl": 2, "leg_fr": 3, "leg_bl": 4, "leg_br": 5, "ear_l": 6,
                   "ear_r": 7, "tail": 8, "neck": 9}, an.PART
assert an.PAIRS["ear"] == ("ear_l", "ear_r") and set(an.PAIRS) == {"front_leg", "back_leg", "ear"}
assert fa.to_blender(1.0, 2.0, 3.0) == (1.0, -2.0, 3.0)

# Part ids per spec part name (paired parts carry both copies' ids).
IDS = {"body": [an.PART["body"]], "head": [an.PART["head"]], "neck": [an.PART["neck"]],
       "tail": [an.PART["tail"]], "front_leg": [an.PART["leg_fl"], an.PART["leg_fr"]],
       "back_leg": [an.PART["leg_bl"], an.PART["leg_br"]], "ear": [an.PART["ear_l"], an.PART["ear_r"]]}


def rot_x(deg: float) -> np.ndarray:
    """Rotation about the creature's x axis in (x, f, z): a negative angle turns +f toward -z (nose
    down)."""
    a = math.radians(deg)
    c, s = math.cos(a), math.sin(a)
    return np.array([[1.0, 0.0, 0.0], [0.0, c, -s], [0.0, s, c]])


class Map:
    """p' = M (p - src) + dst in the creature frame (x, f, z)."""

    def __init__(self, M, src=(0.0, 0.0, 0.0), dst=(0.0, 0.0, 0.0)):
        self.M = np.asarray(M, np.float64)
        self.Mi = np.linalg.inv(self.M)
        self.src = np.asarray(src, np.float64)
        self.dst = np.asarray(dst, np.float64)

    def __call__(self, p):
        return (np.asarray(p, np.float64) - self.src) @ self.M.T + self.dst

    def inv(self, p):
        return (np.asarray(p, np.float64) - self.dst) @ self.Mi.T + self.src

    def vec(self, v):
        w = self.M @ np.asarray(v, np.float64)
        return w / np.linalg.norm(w)


def scale(s, src=(0.0, 0.0, 0.0), dst=(0.0, 0.0, 0.0)) -> Map:
    """Per-axis scale (sx, sf, sz) about src, moved to dst."""
    return Map(np.diag(np.broadcast_to(np.asarray(s, np.float64), (3,))), src, dst)


def rigid(deg: float, src, dst) -> Map:
    """Turn about x by deg about src, then move src to dst."""
    return Map(rot_x(deg), src, dst)


def moved(part: an.Part, m: Map, frame: str, sw: float = 1.0, sh: float = 1.0) -> an.Part:
    """A copy of `part` with every centre through m, half widths times sw and half heights times sh.
    A fixed frame's X and N go through m too; a path frame recomputes N along the moved path (X stays
    the creature's x, which every map here keeps)."""
    st = [(tuple(m(part.c[i])), float(part.w[i] * sw), float(part.ht[i] * sh), float(part.hb[i] * sh),
           float(part.e[i])) for i in range(len(part.c))]
    if frame == "fixed":
        return an.Part(st, "fixed", X=tuple(m.vec(part.X[0])), N=tuple(m.vec(part.N[0])))
    assert np.allclose(m.vec((1.0, 0.0, 0.0)), (1.0, 0.0, 0.0)), "a path-frame part needs x kept"
    return an.Part(st, "path")


def bezier(ctrl, t):
    p0, p1, p2, p3 = (np.asarray(c, np.float64) for c in ctrl)
    t = np.asarray(t, np.float64)[:, None]
    return (1 - t) ** 3 * p0 + 3 * (1 - t) ** 2 * t * p1 + 3 * (1 - t) * t ** 2 * p2 + t ** 3 * p3


def curve_part(ctrl, keys, n: int) -> an.Part:
    """A path-frame loft along the cubic curve ctrl (four creature points), n stations at even arc
    length. keys: [(fraction, w, ht, hb, e)] sizes along the arc, interpolated linearly."""
    t = np.linspace(0.0, 1.0, 2001)
    p = bezier(ctrl, t)
    s = np.concatenate([[0.0], np.cumsum(np.linalg.norm(np.diff(p, axis=0), axis=1))])
    fr = np.linspace(0.0, 1.0, n)
    tt = np.interp(fr * s[-1], s, t)
    c = bezier(ctrl, tt)
    k = np.array(keys, np.float64)
    st = [(tuple(c[i]),) + tuple(float(np.interp(fr[i], k[:, 0], k[:, j])) for j in range(1, 5))
          for i in range(n)]
    return an.Part(st, "path")


def loft_map(src: an.Part, dst: an.Part, P: np.ndarray, keep: float = 0.0) -> np.ndarray:
    """Points P near loft `src` to the same place on loft `dst`: the same lateral and normal offsets
    as fractions of the local half sizes, at the same centre-path length up to `keep` m from the
    root and then stretched evenly over the rest of each path (keep 0: the same fraction)."""
    s, _, xf, nf = src.local(P)
    sd = np.where(s <= keep, s, keep + (s - keep) * (dst.Lc - keep) / max(src.Lc - keep, 1e-9))
    sd = np.clip(sd, 0.0, dst.Lc)
    i = np.clip(np.searchsorted(dst.s, sd, side="right") - 1, 0, len(dst.s) - 2)
    t = ((sd - dst.s[i]) / np.maximum(dst.s[i + 1] - dst.s[i], 1e-9))[:, None]
    lerp = lambda a: a[i] + (a[i + 1] - a[i]) * (t if a.ndim == 2 else t[:, 0])  # noqa: E731
    c, X, N = lerp(dst.c), lerp(dst.X), lerp(dst.N)
    h = np.where(nf > 0, lerp(dst.ht), lerp(dst.hb))
    return c + X * (lerp(dst.w) * xf)[:, None] + N * (h * nf)[:, None]


class RestPaint:
    """Paints each sample at its rest position on the original: rest[name] is a function from the
    variant's creature points to the original's (default: identity). height_scale scales the paint's
    relief (in metres) back to the variant's size."""

    def __init__(self, paint, rest: dict, height_scale: float = 1.0):
        self.paint, self.rest, self.height_scale = paint, rest, height_scale

    def __call__(self, P, part):
        Q = P.copy()
        for name, fn in self.rest.items():
            sel = np.isin(part, IDS[name])
            if sel.any():
                Q[sel] = fn(P[sel])
        out = self.paint(Q, part)
        out["height"] = (out["height"] * self.height_scale).astype(np.float32)
        return out


def mirror_x(fn):
    """A rest map for a paired part: maps |x| and restores the sign, so both copies share one map."""
    def go(P):
        sg = np.where(P[:, 0] < 0, -1.0, 1.0)
        Q = P.copy()
        Q[:, 0] = np.abs(Q[:, 0])
        R = fn(Q)
        R[:, 0] *= sg
        return R
    return go


def neck_base(spec: dict, pivot) -> tuple:
    """(f, z) on the pivot -> muzzle-tip axis at the largest projection of any neck surface point
    inside the body, so the head weight V is 0 on the buried neck root and its whole exit ring."""
    tip = spec["head"].c[-1][1:]
    piv = np.asarray(pivot, np.float64)
    a = (tip - piv) / np.linalg.norm(tip - piv)
    neck = spec["neck"]
    pts = []
    for i in range(len(neck.c) - 1):
        for u in np.linspace(0.0, 1.0, 9):
            for k in range(48):
                phi = 2 * np.pi * k / 48
                pts.append(neck.point(i, phi) * (1 - u) + neck.point(i + 1, phi) * u)
    pts = np.array(pts)
    _, val, _, _ = spec["body"].local(pts)
    t = (pts[:, 1:] - piv) @ a
    b = piv + a * float(t[val < 1.0].max())
    return (float(b[0]), float(b[1]))


def measures(spec: dict) -> dict:
    """Withers (the body's top) and muzzle tip to rump (the body's back pole), in metres."""
    body = spec["body"]
    top = max(float(body.point(i, 0.0)[2]) for i in range(len(body.c)))
    return {"withersM": round(top, 3),
            "muzzleToRumpM": round(float(spec["head"].c[-1][1] - body.c[-1][1]), 3)}


def reach(spec: dict) -> float:
    """Largest horizontal distance (x, f) from the pivot to any spec surface sample, for the scatter
    footprint and the P3 wander."""
    r = 0.0
    for name, p in spec.items():
        for i in range(len(p.c)):
            for k in range(32):
                q = p.point(i, 2 * math.pi * k / 32)
                xs = (q[0], -q[0]) if name in an.PAIRS else (q[0],)
                for x in xs:
                    r = max(r, math.hypot(x, q[1]))
    return r


def build(piece, spec, lods, motion, paint, out, skip_bake, ao_dist, real, gait, file, extra=None):
    """The `_animals` pipeline for a variant spec: buried check, atlas, LODs, maps, export and the
    motion check, exactly as the original generators run it."""
    an.check_buried(spec, lods)
    mat = fa.setup(piece)
    atlas = an.make_atlas(spec)
    objs = [an.build_lod(piece, i, spec, lods, atlas, motion, mat) for i in range(3)]
    texel = an.density(objs[0])
    for o in objs[1:]:
        o.hide_render = True
    stats = an.paint_maps(objs[0], spec, paint, out, piece, skip_bake, ao_dist, atlas.sig)
    for o in objs[1:]:
        o.hide_render = False
    extras = an.make_extras(spec, motion, real, gait)
    extras.update(extra or {})
    print(f"{piece}: real {real}, reach {reach(spec):.3f} m")
    return an.export_piece(piece, objs, out, file, extras, texel, atlas.sig, stats, skip_bake, ao_dist, motion)
