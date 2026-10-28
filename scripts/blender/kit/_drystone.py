"""Dry-stone field walls (B2, owner A31 2026-10-27; refs: assets-source/refs/walls_hedges, ref-1, ref-3
and ref-4): dry granite walls 1.1 m tall and 0.7 m wide at the foot, battered faces of rough coursed
fieldstone with no mortar, and a cope of flat stones set upright on edge along the top, laid along
shared tile edges as runtime field boundaries (`hex_core::boundary`).

Pieces (Blender frame: x along the edge, z up; runtime (x, z, -y)):
- a run: 4 m along x, centred on the pivot, closed at both ends (so a gap beside it shows a built
  end, not a hollow);
- an end, a bend and a junction: one, two or three 2 m arms leaving the pivot, which sits on the hex
  corner, at 0, 120 and 240 degrees (the hex's 120 degree corners, never the refs' right angles).
  Where two arms meet, their faces are mitred on the bisector, so the faces meet edge to edge round
  the corner; an end piece stops square at the corner;
- the gateway: a 4 m run of two short wall stubs, two tall granite gateposts and a closed timber
  five-bar gate between them (ref-1).

Body: each arm's two faces lean in from FOOT_HW at the ground to TOP_HW at BODY_H (the batter),
reach BURY below the ground (the meadow relief dips to about -0.25 m), and carry a little bulge so
the silhouette is not ruled. Cope: upright slabs across the wall top, each leaning and twisting a
little, to WALL_H. LOD1 halves the face grid and keeps every other cope slab twice as thick; LOD2 is
the bare battered body with a few chunky cope blocks.

Look: one unique `stone` slot. Its maps are drawn in numpy (no Cycles bake), in the granite palette
of the boulders and rocks (`_rock.granite_material`: a #8c8982-#aba79e mottle, dark #3b3936 and pale
#c4bfb3 flecks, pale grey-green and mustard lichen, dark moss low in the joints): each face is laid
as courses of rough stones with dark joints, the height field gives the normal map and the joint
AO (ORM.R; masks.A = 1). The faces map in metres at TEXEL px/m into strips of the 1024 atlas (a face
longer than a strip is cut into chunks drawn from one continuous image, so the stones run on across
the cut); every cope slab samples one of four shared slab tiles; the end faces and gateposts their
own regions. The gate is the `timber` slot on the world-space plaster set tinted weathered grey-brown
(the fence's route), its masks constant.

Collider: one box per arm (disjoint parts, so the walker is not blocked in the corner between two
arms); the gateway one box over its whole 4 m, gate shut."""
from __future__ import annotations

import math
import os
import pathlib
import random

import bmesh
import bpy
import numpy as np
from mathutils import Vector

from lib import bake, budgets, collider, export, geom, materials, scene, uv

ATLAS, NORMAL_SIZE, MASK_SIZE = 1024, 512, 256
FOOT_HW, TOP_HW, BODY_H, WALL_H, BURY = 0.35, 0.235, 0.84, 1.10, 0.30
ARM = 2.0
SLANT = math.hypot(1.0, (FOOT_HW - TOP_HW) / BODY_H)
GUT = 8  # gutter px round every atlas region
V_LO = -0.03  # face image rows start this far (m, along the slant) below the ground line
ROWS = 3
COPE_TILE = (0.56, 0.34)  # m (across the wall, up)
POST_M = (1.5, 0.42)  # (along u: up the post, across)


def set_texel(texel: float) -> None:
    """Lays the atlas out for `texel` px per metre on every stone face (the gate's floor is a
    median of 256 per material). 270 everywhere but the junction, whose six mitred faces (about
    1.86 m each) only fit three face rows at 258."""
    global TEXEL, ROW_PX, COL_A, COPE_Y0, COPE_PX, END_REGION, END_M, POST_REGION, TIMBER_REGION
    TEXEL = float(texel)
    ROW_PX = int(math.ceil((BODY_H * SLANT - V_LO + 0.03) * TEXEL)) + 2 * GUT
    COL_A = int(2.0 * TEXEL) + 2 * GUT + 4
    COPE_Y0 = ROWS * ROW_PX
    COPE_PX = (int(COPE_TILE[0] * TEXEL) + 2 * GUT, int(COPE_TILE[1] * TEXEL) + 2 * GUT)
    END_REGION = (4 * COPE_PX[0], COPE_Y0)  # (x0, y0): an end face region, END_M wide and tall
    END_M = ((ATLAS - 4 * COPE_PX[0] - 2 * GUT) / TEXEL, (ATLAS - COPE_Y0 - 2 * GUT) / TEXEL)
    POST_REGION = (0, COPE_Y0 + COPE_PX[1])  # post sides, mapped with u up the post
    TIMBER_REGION = (POST_REGION[0] + int(POST_M[0] * TEXEL) + 3 * GUT, COPE_Y0 + COPE_PX[1] + GUT, 48)
    assert TEXEL >= 257, TEXEL
    assert ROWS * ROW_PX + COPE_PX[1] + int(POST_M[1] * TEXEL) + 2 * GUT <= ATLAS, (ROW_PX, COPE_PX)
    assert TIMBER_REGION[0] + TIMBER_REGION[2] <= END_REGION[0] and TIMBER_REGION[1] + 48 <= ATLAS
    assert END_M[1] >= 0.6, END_M


set_texel(270.0)

# The granite palette (sRGB), from `_rock.granite_material`.
GRANITE_LO, GRANITE_HI = "#8c8982", "#aba79e"
FLECK_DARK, FLECK_PALE = "#3b3936", "#c4bfb3"
LICHEN_GREY, LICHEN_MUSTARD, MOSS, JOINT = "#a7ab90", "#b39c45", "#3a4824", "#46413a"


def hexrgb(h: str) -> np.ndarray:
    return np.array([int(h[i:i + 2], 16) for i in (1, 3, 5)], np.float32) / 255


def half_w(z: float) -> float:
    """Face half width at height z (the batter runs on below the ground)."""
    return FOOT_HW - (FOOT_HW - TOP_HW) * z / BODY_H


# --- numpy texture drawing (Blender pixel order: row 0 = v 0) ---

def value_noise(h: int, w: int, cell: float, rng: np.random.Generator) -> np.ndarray:
    """Smooth value noise in [0, 1] on an h x w grid with lattice spacing `cell` px."""
    gh, gw = int(h / cell) + 3, int(w / cell) + 3
    g = rng.random((gh, gw)).astype(np.float32)
    ys, xs = np.arange(h, dtype=np.float32) / cell, np.arange(w, dtype=np.float32) / cell
    y0, x0 = ys.astype(int), xs.astype(int)
    fy, fx = ys - y0, xs - x0
    fy, fx = fy * fy * (3 - 2 * fy), fx * fx * (3 - 2 * fx)
    a = g[y0][:, x0]
    b = g[y0][:, x0 + 1]
    c = g[y0 + 1][:, x0]
    d = g[y0 + 1][:, x0 + 1]
    top = a + (b - a) * fx[None]
    bot = c + (d - c) * fx[None]
    return top + (bot - top) * fy[:, None]


def fbm(h: int, w: int, cell: float, rng: np.random.Generator, octaves: int = 3) -> np.ndarray:
    out, amp, tot = np.zeros((h, w), np.float32), 1.0, 0.0
    for k in range(octaves):
        out += amp * value_noise(h, w, max(cell / 2 ** k, 1.5), rng)
        tot += amp
        amp *= 0.5
    return out / tot


def granite(h: int, w: int, rng: np.random.Generator, tone: np.ndarray) -> np.ndarray:
    """The boulders' granite: a mottle, dark and pale flecks, times a per-texel stone tone."""
    mottle = np.clip((fbm(h, w, 0.33 * TEXEL, rng) - 0.35) / 0.3, 0, 1)
    col = hexrgb(GRANITE_LO) + (hexrgb(GRANITE_HI) - hexrgb(GRANITE_LO)) * mottle[..., None]
    col = col * tone[..., None]
    speck = rng.random((h, w)).astype(np.float32)
    fine = value_noise(h, w, 2.2, rng)
    dark = (speck < 0.07) | (fine < 0.12)
    pale = (speck > 0.94) | (fine > 0.9)
    col[dark] = col[dark] * 0.35 + hexrgb(FLECK_DARK) * 0.65
    col[pale] = col[pale] * 0.4 + hexrgb(FLECK_PALE) * 0.6
    return col


def lichen(col: np.ndarray, rough: np.ndarray, on_stone: np.ndarray, up: np.ndarray, rng: np.random.Generator,
           amount: float) -> None:
    """Pale grey-green and mustard lichen blobs on stone, more of them higher up (`up` in [0, 1])."""
    h, w = on_stone.shape
    blob = fbm(h, w, 0.12 * TEXEL, rng, 4)
    grey = on_stone & (blob > 0.68 - 0.08 * up * amount)
    must = on_stone & (fbm(h, w, 0.05 * TEXEL, rng, 3) > 0.83 - 0.04 * up * amount)
    col[grey] = col[grey] * 0.5 + hexrgb(LICHEN_GREY) * 0.5
    col[must] = col[must] * 0.55 + hexrgb(LICHEN_MUSTARD) * 0.45
    rough[grey | must] = 0.93


def stone_face(seed: str, length: float, big: bool = False) -> dict:
    """A dry-stone face image `length` m long (+ gutters) and ROW_PX tall: courses of rough stones
    with dark joints. Returns albedo (sRGB), height (m), ao, roughness, all (ROW_PX, W)."""
    rng = np.random.default_rng(int.from_bytes(seed.encode()[:8].ljust(8, b"0"), "little") ^ len(seed) * 7919)
    pr = random.Random(seed)
    w = int(math.ceil(length * TEXEL)) + 2 * GUT
    h = ROW_PX
    xs = (np.arange(w, dtype=np.float32) - GUT + 0.5) / TEXEL  # metres along the face
    vs = (np.arange(h, dtype=np.float32) - GUT + 0.5) / TEXEL + V_LO  # metres up the slant
    top = BODY_H * SLANT
    # Courses: heights from the ground up, the bottom ones deeper.
    bounds, z = [V_LO - 0.2], 0.0
    while z < top - 0.12:
        ch = pr.uniform(0.15, 0.26) if z < 0.25 else pr.uniform(0.09, 0.21)
        z = min(z + ch, top)
        bounds.append(z)
    bounds[-1] = top + 0.2
    nb = len(bounds)
    wob = fbm(1, w, 0.25 * TEXEL, rng, 2)[0]
    zb = np.stack([np.full(w, b, np.float32) + (wob - 0.5) * 0.05 * (0 < i < nb - 1) for i, b in enumerate(bounds)])
    zb = np.sort(zb, axis=0)
    above = vs[None, :, None] >= zb[:, None, :]  # (nb, h, w)
    course = np.clip(above.sum(0) - 1, 0, nb - 2)
    lo = zb[course, np.arange(w)[None, :]]
    hi = zb[course + 1, np.arange(w)[None, :]]
    dz = np.minimum(vs[:, None] - lo, hi - vs[:, None])
    dx = np.full((h, w), 9.0, np.float32)
    stone_id = np.zeros((h, w), np.int32)
    tone = np.ones((h, w), np.float32)
    tilt = np.zeros((h, w), np.float32)
    jut = np.zeros((h, w), np.float32)
    next_id = 0
    for k in range(nb - 1):
        m = course == k
        if not m.any():
            continue
        x, stones = -pr.uniform(0.0, 0.3), []
        while x < length + 0.3:
            ln = pr.uniform(0.28, 0.66) if (k < 2 or big) else pr.uniform(0.14, 0.56)
            stones.append(x)
            x += ln
        stones.append(x)
        sl = np.array([pr.uniform(-0.32, 0.32) for _ in stones], np.float32)
        zc = (bounds[k] + bounds[k + 1]) / 2
        yy, xx = np.nonzero(m)
        xb = np.array(stones, np.float32)[None, :] + sl[None, :] * (vs[yy][:, None] - zc)
        j = np.clip((xs[xx][:, None] >= xb).sum(1) - 1, 0, len(stones) - 2)
        left = xb[np.arange(len(j)), j]
        right = xb[np.arange(len(j)), j + 1]
        dx[yy, xx] = np.minimum(xs[xx] - left, right - xs[xx])
        # About one stone in four is two thinner stones laid one on the other (and one in eight a
        # small pinning stone beside a joint), so the courses do not read as dressed brick rows.
        split = np.array([pr.random() < 0.28 for _ in stones])
        zs_ = np.array([zc + pr.uniform(-0.25, 0.25) * (bounds[k + 1] - bounds[k]) for _ in stones], np.float32)
        tilt_s = np.array([pr.uniform(-0.12, 0.12) for _ in stones], np.float32)
        mid = (left + right) / 2
        zsplit = zs_[j] + tilt_s[j] * (xs[xx] - mid)
        on = split[j]
        dzs = np.abs(vs[yy] - zsplit)
        dz[yy[on], xx[on]] = np.minimum(dz[yy[on], xx[on]], dzs[on])
        upper = on & (vs[yy] > zsplit)
        ids = next_id + j * 2 + upper
        stone_id[yy, xx] = ids
        next_id += 2 * len(stones)
        tones = np.array([pr.uniform(0.78, 1.06) for _ in range(2 * len(stones))], np.float32)
        tilts = np.array([pr.uniform(-1, 1) for _ in stones], np.float32)
        juts = np.array([0.03 if pr.random() < 0.07 else pr.uniform(-0.008, 0.012) for _ in stones], np.float32)
        tone[yy, xx] = tones[j * 2 + upper]
        tilt[yy, xx] = tilts[j] * (xs[xx] - (left + right) / 2) * 0.03
        jut[yy, xx] = juts[j]
    # Rough outlines: the distance to a stone's edge wanders by a few centimetres, and its corners
    # are rounded, so the stones read as split fieldstone, not dressed blocks.
    rough_edge = 0.022 * (fbm(h, w, 0.07 * TEXEL, rng, 3) - 0.5) + 0.01 * (fbm(h, w, 0.025 * TEXEL, rng, 2) - 0.5)
    rc = 0.05
    corner = (dx < rc) & (dz < rc)
    d = np.where(corner, rc - np.hypot(rc - dx, rc - dz), np.minimum(dx, dz)) + rough_edge
    jw = 0.007 + 0.007 * fbm(h, w, 0.05 * TEXEL, rng, 2)
    joint = d < jw
    edge = np.clip((d - jw) / 0.07, 0, 1)
    height = np.where(joint, -0.03, 0.035 * edge ** 0.5 + tilt + jut + 0.005 * (fbm(h, w, 0.02 * TEXEL, rng, 2) - 0.5))
    col = granite(h, w, rng, tone)
    # Some fieldstones are warmer or cooler than the granite's own mottle.
    warm = np.where(stone_id % 5 == 1, 1.0, 0.0)[..., None] * np.array([0.04, 0.015, -0.02], np.float32)
    cool = np.where(stone_id % 7 == 3, 1.0, 0.0)[..., None] * np.array([-0.02, -0.005, 0.015], np.float32)
    col = col + warm + cool
    rough = np.where(joint, 0.95, 0.84 + 0.06 * fbm(h, w, 0.1 * TEXEL, rng, 2)).astype(np.float32)
    up = np.clip(vs / top, 0, 1)[:, None] * np.ones((1, w), np.float32)
    lichen(col, rough, ~joint & (edge > 0.2), up, rng, 1.0)
    jcol = np.where((up < 0.45)[..., None] & (fbm(h, w, 0.08 * TEXEL, rng, 2) > 0.55)[..., None],
                    hexrgb(MOSS), hexrgb(JOINT))
    col = np.where(joint[..., None], jcol * (0.8 + 0.3 * fbm(h, w, 0.03 * TEXEL, rng, 1))[..., None], col)
    ao = np.where(joint, 0.42, 0.62 + 0.38 * edge ** 0.5) * (0.8 + 0.2 * np.clip(vs / 0.3, 0, 1))[:, None]
    return {"albedo": np.clip(col, 0, 1).astype(np.float32), "height": height.astype(np.float32),
            "ao": ao.astype(np.float32), "rough": rough}


def slab_tile(seed: str, w_m: float, h_m: float) -> dict:
    """One cope slab texture (w_m x h_m + gutters): a single granite stone with lichen on its upper
    part and darkened, rounded edges toward its border."""
    rng = np.random.default_rng(int.from_bytes(seed.encode()[:8].ljust(8, b"0"), "little") + 17)
    w, h = int(w_m * TEXEL) + 2 * GUT, int(h_m * TEXEL) + 2 * GUT
    pr = random.Random(seed)
    col = granite(h, w, rng, np.full((h, w), pr.uniform(0.9, 1.06), np.float32))
    rough = (0.84 + 0.06 * fbm(h, w, 0.1 * TEXEL, rng, 2)).astype(np.float32)
    yy, xx = np.mgrid[0:h, 0:w].astype(np.float32)
    up = np.clip(yy / h, 0, 1)
    lichen(col, rough, np.ones((h, w), bool), up, rng, 1.6)
    height = 0.012 * (fbm(h, w, 0.06 * TEXEL, rng, 3) - 0.5)
    ao = np.full((h, w), 0.92, np.float32)
    return {"albedo": np.clip(col, 0, 1).astype(np.float32), "height": height.astype(np.float32), "ao": ao,
            "rough": rough}


class Atlas:
    """The 1024 stone atlas: face strips (rows of an A and a B column), cope slab tiles, an end face
    region, a gatepost region and a timber masks patch. `put` copies a drawn image into a rect."""

    def __init__(self) -> None:
        self.alb = np.zeros((ATLAS, ATLAS, 3), np.float32)
        self.alb[:] = hexrgb(GRANITE_LO)
        self.hgt = np.zeros((ATLAS, ATLAS), np.float32)
        self.ao = np.ones((ATLAS, ATLAS), np.float32)
        self.rough = np.full((ATLAS, ATLAS), 0.88, np.float32)
        self.snow = np.full((ATLAS, ATLAS), 0.3, np.float32)
        # Face strips: [x cursor px, x end px, y0 px] per row and column, filled left to right.
        self.free = [[x0, x1, r * ROW_PX] for r in range(ROWS) for x0, x1 in ((0, COL_A), (COL_A, ATLAS))]

    def strip(self, length: float) -> tuple[int, int, float]:
        """Room for a chunk of up to `length` m: the inner origin (x0, y0) and the metres granted.
        A strip that holds the whole length is preferred; otherwise the roomiest one is used."""
        def room(s):
            return (s[1] - s[0] - 2 * GUT - 1) / TEXEL
        fits = [s for s in self.free if room(s) >= length]
        s = fits[0] if fits else max(self.free, key=room)
        got = min(length, room(s))
        assert got >= min(length, 0.02), f"face strips full ({length:.2f} m left)"
        x0 = s[0] + GUT
        s[0] += int(math.ceil(got * TEXEL)) + 2 * GUT + 1
        return x0, s[2] + GUT, got

    def put(self, img: dict, x0: int, y0: int, src_x0: int, w: int, snow: float) -> None:
        """Copies columns [src_x0 - GUT, src_x0 + w + GUT) of img into the atlas at inner origin
        (x0, y0); img's rows include its own gutters."""
        h = img["albedo"].shape[0]
        # Edge-pad a few columns, so a chunk rounded a pixel past either end of its image still fills.
        img = {k: np.pad(v, ((0, 0), (4, 4)) + ((0, 0),) * (v.ndim - 2), mode="edge") for k, v in img.items()}
        src_x0 += 4
        sx = slice(src_x0 - GUT, src_x0 + w + GUT)
        dst = (slice(y0 - GUT, y0 - GUT + h), slice(x0 - GUT, x0 + w + GUT))
        self.alb[dst] = img["albedo"][:, sx]
        self.hgt[dst] = img["height"][:, sx]
        self.ao[dst] = img["ao"][:, sx]
        self.rough[dst] = img["rough"][:, sx]
        self.snow[dst] = snow


# --- geometry ---

class Mesh:
    """A bmesh with a UV layer and a slot index layer; faces from (verts, uvs) in pixels."""

    def __init__(self) -> None:
        self.bm = bmesh.new()
        self.uv = self.bm.loops.layers.uv.new("UVMap")

    def face(self, pts: list[Vector], uvs_px: list[tuple[float, float]], slot: int = 0) -> None:
        vs = [self.bm.verts.new(p) for p in pts]
        f = self.bm.faces.new(vs)
        f.material_index = slot
        for lp, (u, v) in zip(f.loops, uvs_px):
            lp[self.uv].uv = (u / ATLAS, v / ATLAS)

    def object(self, name: str, mats: list) -> bpy.types.Object:
        bmesh.ops.triangulate(self.bm, faces=self.bm.faces[:])
        me = bpy.data.meshes.new(name)
        for m in mats:
            me.materials.append(m)
        self.bm.to_mesh(me)
        self.bm.free()
        for p in me.polygons:
            p.use_smooth = False
        return scene.link(bpy.data.objects.new(name, me))


def arm_frames(kind: str, arms: tuple[float, ...]) -> list[dict]:
    """Per arm: start a, direction d, length, and per side (+1 left, -1 right) the mitre factor k
    (the face starts at t = k * half width): cot of half the angle to the neighbouring arm on that
    side, or None for a square end at t = 0."""
    if kind in ("run", "gate"):
        return [{"a": Vector((-ARM, 0, 0)), "d": Vector((1, 0, 0)), "len": 2 * ARM, "k": {1: None, -1: None},
                 "far_end": True}]
    out = []
    angs = sorted(arms)
    for a in angs:
        d = Vector((math.cos(math.radians(a)), math.sin(math.radians(a)), 0.0))
        k = {}
        for side in (1, -1):
            others = [(o - a) % 360 if side == 1 else (a - o) % 360 for o in angs if o != a]
            if not others:
                k[side] = None
            else:
                phi = min(others)
                k[side] = 1.0 / math.tan(math.radians(phi) / 2)
        out.append({"a": Vector((0, 0, 0)), "d": d, "len": ARM, "k": k, "far_end": False})
    return out


def face_rows(lod: int) -> list[float]:
    if lod == 0:
        return [-BURY, 0.0, 0.21, 0.42, 0.63, BODY_H]
    if lod == 1:
        return [-BURY, 0.0, 0.42, BODY_H]
    return [-BURY, 0.0, BODY_H]


def v_of(z: float) -> float:
    """Slant metres up the face image for height z; the buried row squeezes into the gutter."""
    if z >= 0.0:
        return z * SLANT
    return V_LO * 0.6 * (z / -BURY)


def bulge(seed: str, t: float, z: float, interior: bool) -> float:
    if not interior:
        return 0.0
    r = random.Random(f"{seed}:{round(t, 3)}:{round(z, 3)}")
    return r.uniform(-0.012, 0.016)


def build_body(m: Mesh, seed: str, frames: list[dict], lod: int, atlas: Atlas | None, faces_px: dict,
               end_square: bool) -> None:
    """The battered faces of every arm (mitred where arms meet), square end faces, and the top."""
    rows = face_rows(lod)
    col_step = {0: 0.25, 1: 0.5, 2: 9.0}[lod]
    tops = []
    for ai, fr in enumerate(frames):
        a, d, ln = fr["a"], fr["d"], fr["len"]
        perp = Vector((-d.y, d.x, 0))
        for side in (1, -1):
            k = fr["k"][side]
            t_start = (lambda z, k=k: (k or 0.0) * half_w(z))
            key = (ai, side)
            t_lo = min(t_start(z) for z in rows)
            # The face's UV chunks (from the LOD0 atlas layout, shared by every LOD).
            chunks = faces_px[key]
            fixed = sorted({c["t0"] for c in chunks[1:]} | {ln} | {
                t for t in np.arange(t_lo + col_step, ln - 0.05, col_step).tolist()
                if t > max(t_start(z) for z in rows) + 0.04})
            cols = [None] + fixed  # None = this row's own start
            for ri in range(len(rows) - 1):
                z0, z1 = rows[ri], rows[ri + 1]
                for ci in range(len(cols) - 1):
                    ta = [t_start(z0), t_start(z1)] if cols[ci] is None else [cols[ci]] * 2
                    tb = [cols[ci + 1]] * 2
                    tm = (ta[0] + tb[0]) / 2
                    ch = next(c for c in chunks if c["t0"] - 1e-6 <= tm <= c["t1"] + 1e-6)
                    pts, uvs = [], []
                    for (t, z) in ((ta[0], z0), (tb[0], z0), (tb[1], z1), (ta[1], z1)):
                        interior = 0.0 < z < BODY_H and t_lo + 0.05 < t < ln - 0.05 and \
                            all(abs(t - c["t0"]) > 1e-6 for c in chunks[1:])
                        hw = half_w(z) + bulge(seed, t * (ai + 1), z * side, interior and lod == 0)
                        pts.append(a + d * t + perp * side * hw + Vector((0, 0, z)))
                        u = ch["x0"] + (t - ch["t0"]) * TEXEL if side == 1 else ch["x0"] + (ch["t1"] - t) * TEXEL
                        uvs.append((u, ch["y0"] + (v_of(z) - V_LO) * TEXEL))
                    if side == 1:  # (+t, then up) winds inward on the left face
                        pts.reverse()
                        uvs.reverse()
                    m.face(pts, uvs)
            # The square end at t = 0 (an end piece's corner end, a run's ends).
            if k is None and side == 1 and end_square:
                end_face(m, a, d, perp, 0.0, flip=True)
        if fr["far_end"]:
            end_face(m, a, d, perp, ln, flip=False)
        # Top of the body (under the cope), one quad per arm from its mitres to its end.
        tl = (fr["k"][1] or 0.0) * TOP_HW
        tr = (fr["k"][-1] or 0.0) * TOP_HW
        quad = [a + d * tr - perp * TOP_HW, a + d * ln - perp * TOP_HW, a + d * ln + perp * TOP_HW,
                a + d * tl + perp * TOP_HW]
        tops.append((quad, [a + d * tl + perp * TOP_HW, a + d * tr - perp * TOP_HW]))
    # The tops sit under the cope: they sample a patch of slab tile 0.
    for quad, _ in tops:
        pts = [p + Vector((0, 0, BODY_H)) for p in quad]
        m.face(pts, [(GUT + 10, COPE_Y0 + GUT + 10), (GUT + 120, COPE_Y0 + GUT + 10), (GUT + 120, COPE_Y0 + GUT + 60),
                     (GUT + 10, COPE_Y0 + GUT + 60)])
    starts = []
    for _, s in tops:
        for p in s:
            if all((p - q).length > 1e-4 for q in starts):
                starts.append(p)
    if len(starts) >= 3 and all(fr["a"].length < 1e-6 for fr in frames):  # a junction's middle
        starts.sort(key=lambda p: math.atan2(p.y, p.x))
        m.face([p + Vector((0, 0, BODY_H)) for p in starts],
               [(GUT + 20 + 40 * math.cos(i * 2.1), COPE_Y0 + GUT + 40 + 25 * math.sin(i * 2.1)) for i in range(len(starts))])


def end_face(m: Mesh, a: Vector, d: Vector, perp: Vector, t: float, flip: bool) -> None:
    """The square end of an arm at t: a trapezoid from -BURY to BODY_H, in the end region."""
    ex, ey = END_REGION[0] + GUT, END_REGION[1] + GUT
    pts, uvs = [], []
    for s, z in ((-1, -BURY), (1, -BURY), (1, BODY_H), (-1, BODY_H)):
        pts.append(a + d * t + perp * s * half_w(z) + Vector((0, 0, z)))
        uvs.append((ex + (END_M[0] / 2 + s * half_w(z) * 0.98) * TEXEL, ey + max(0.0, (z + 0.05) * TEXEL * 0.98)))
    if flip:
        pts.reverse()
        uvs.reverse()
    m.face(pts, uvs)


def box(m: Mesh, c: Vector, axes: tuple[Vector, Vector, Vector], half: tuple[float, float, float],
        region: tuple[int, int], bottom: bool = False) -> None:
    """A box (centre c, unit axes, half extents) with metric box-projected UVs into a region whose
    inner origin is `region`: faces along axis 0 use (axis 1, axis 2), along axis 1 use (axis 0,
    axis 2), the top uses (axis 1, axis 0)."""
    ax, ay, az = axes
    hx, hy, hz = half

    def P(i, j, k):
        return c + ax * (i * hx) + ay * (j * hy) + az * (k * hz)

    x0, y0 = region

    def U(p, q, wp, wq):
        return (x0 + (p + 1) * wp * TEXEL, y0 + (q + 1) * wq * TEXEL)

    faces = [
        ([P(1, -1, -1), P(1, 1, -1), P(1, 1, 1), P(1, -1, 1)], lambda i, j, k: U(j, k, hy, hz)),
        ([P(-1, 1, -1), P(-1, -1, -1), P(-1, -1, 1), P(-1, 1, 1)], lambda i, j, k: U(-j, k, hy, hz)),
        ([P(-1, 1, -1), P(-1, 1, 1), P(1, 1, 1), P(1, 1, -1)], lambda i, j, k: U(i, k, hx, hz)),
        ([P(1, -1, -1), P(1, -1, 1), P(-1, -1, 1), P(-1, -1, -1)], lambda i, j, k: U(-i, k, hx, hz)),
        ([P(-1, -1, 1), P(1, -1, 1), P(1, 1, 1), P(-1, 1, 1)], lambda i, j, k: U(j, i, hy, hx)),
    ]
    idx = [[(1, -1, -1), (1, 1, -1), (1, 1, 1), (1, -1, 1)], [(-1, 1, -1), (-1, -1, -1), (-1, -1, 1), (-1, 1, 1)],
           [(-1, 1, -1), (-1, 1, 1), (1, 1, 1), (1, 1, -1)], [(1, -1, -1), (1, -1, 1), (-1, -1, 1), (-1, -1, -1)],
           [(-1, -1, 1), (1, -1, 1), (1, 1, 1), (-1, 1, 1)]]
    if bottom:
        faces.append(([P(-1, -1, -1), P(-1, 1, -1), P(1, 1, -1), P(1, -1, -1)], lambda i, j, k: U(j, i, hy, hx)))
        idx.append([(-1, -1, -1), (-1, 1, -1), (1, 1, -1), (1, -1, -1)])
    for (pts, f), ii in zip(faces, idx):
        m.face(pts, [f(*q) for q in ii])


def cope_slabs(seed: str, frames: list[dict], lod: int, centre: bool) -> list[dict]:
    """Upright cope slabs along every arm's top: centre, axes and half extents; a centre block over
    the corner when arms meet there."""
    rng = random.Random(f"{seed}:cope")
    out = []
    for fr in frames:
        a, d, ln = fr["a"], fr["d"], fr["len"]
        perp = Vector((-d.y, d.x, 0))
        t = 0.0 if fr["far_end"] or not centre else 0.3
        if lod == 2:
            n = 1 if ln < 1.0 else (2 if ln <= ARM + 1e-6 else 4)
            step = (ln - t) / n
            for i in range(n):
                tc = t + step * (i + 0.5)
                out.append({"c": a + d * tc + Vector((0, 0, BODY_H + 0.1)), "ax": (d, perp, Vector((0, 0, 1))),
                            "half": (step / 2, TOP_HW + 0.02, 0.13)})
            continue
        slabs = []
        while t < ln - 0.04:
            th = rng.uniform(0.08, 0.15)
            th = min(th, ln - t)
            slabs.append((t, th))
            t += th + rng.uniform(0.0, 0.015)
        if lod == 1:
            slabs = [(slabs[i][0], sum(s[1] for s in slabs[i:i + 2]) + 0.01) for i in range(0, len(slabs), 2)]
        for t0, th in slabs:
            th = min(th, ln - t0)
            hgt = rng.uniform(0.24, 0.29)
            lean = math.radians(rng.uniform(-7, 7))
            yaw = math.radians(rng.uniform(-5, 5))
            dd = (d * math.cos(yaw) + perp * math.sin(yaw))
            up = (Vector((0, 0, 1)) * math.cos(lean) + dd * math.sin(lean)).normalized()
            dd = (dd - up * dd.dot(up)).normalized()
            pp = up.cross(dd).normalized()
            wid = TOP_HW + rng.uniform(0.01, 0.04)
            c = a + d * (t0 + th / 2) + Vector((0, 0, BODY_H - 0.03 + hgt / 2))
            out.append({"c": c, "ax": (dd, pp, up), "half": (th / 2, wid, hgt / 2)})
    if centre:
        ang = sum(math.atan2(f["d"].y, f["d"].x) for f in frames) / len(frames)
        dd = Vector((math.cos(ang), math.sin(ang), 0))
        out.append({"c": Vector((0, 0, BODY_H + 0.1)), "ax": (dd, Vector((-dd.y, dd.x, 0)), Vector((0, 0, 1))),
                    "half": (0.26, 0.26, 0.14)})
    return out


def post(m: Mesh, c: Vector, lod: int) -> None:
    """A granite gatepost: 0.42 m square, 1.45 m tall with BURY below, its top lightly domed."""
    x0, y0 = POST_REGION[0] + GUT, POST_REGION[1] + GUT
    hw, z0, z1 = 0.21, -0.1, 1.38
    corners = [Vector((sx * hw, sy * hw, 0)) for sx, sy in ((-1, -1), (1, -1), (1, 1), (-1, 1))]
    rows = {0: 6, 1: 2, 2: 1}[lod]
    zs = [z0 + (z1 - z0) * r / rows for r in range(rows + 1)]

    def at(k: int, z: float) -> Vector:
        # Rough-hewn: interior rows bulge or dip a little per corner (LOD0 only).
        wob = 0.0 if lod or z in (z0, z1) else random.Random(f"post:{c.x:.2f}:{k}:{z:.3f}").uniform(-0.02, 0.025)
        return c + corners[k] * (1.0 + wob / hw) + Vector((0, 0, z))

    for i in range(4):
        j = (i + 1) % 4
        for r in range(rows):
            za, zb = zs[r], zs[r + 1]
            ua, ub = (za - z0) * TEXEL, (zb - z0) * TEXEL
            m.face([at(i, za), at(j, za), at(j, zb), at(i, zb)],
                   [(x0 + ua, y0), (x0 + ua, y0 + 0.42 * TEXEL), (x0 + ub, y0 + 0.42 * TEXEL), (x0 + ub, y0)])
    if lod == 2:
        m.face([at(k, z1) for k in range(4)], [(x0 + 10, y0 + 10), (x0 + 60, y0 + 10), (x0 + 60, y0 + 60),
                                              (x0 + 10, y0 + 60)])
        return
    apex = c + Vector((0, 0, z1 + 0.08))
    for i in range(4):
        m.face([at(i, z1), at((i + 1) % 4, z1), apex], [(x0 + 10, y0 + 10), (x0 + 60, y0 + 10), (x0 + 35, y0 + 50)])


def gate(m: Mesh, lod: int) -> None:
    """The closed five-bar gate between the posts' inner faces (x in +-GATE_HALF), slot 1."""
    tx, ty, ts = TIMBER_REGION
    reg = (tx, ty)

    def beam(p: Vector, q: Vector, w: float, h: float, segs: int = 1) -> None:
        if segs > 1:
            # LOD0 timbers in short lengths, each sagging a few millimetres (weathered, hand-made).
            for s in range(segs):
                a, b = p.lerp(q, s / segs), p.lerp(q, (s + 1) / segs)
                sag = Vector((0, 0, -0.006 * math.sin(math.pi * (s + 0.5) / segs)))
                beam(a + sag, b + sag, w, h)
            return
        d = (q - p)
        ln = d.length
        d.normalize()
        up = Vector((0, 0, 1))
        side = up.cross(d).normalized() if abs(d.z) < 0.99 else Vector((0, 1, 0))
        up = d.cross(side).normalized()
        # Timber UVs only carry masks (constant there): squeeze the metric box UVs into the patch.
        before = len(m.bm.faces)
        box(m, (p + q) / 2, (d, side, up), (ln / 2, w / 2, h / 2), reg, bottom=True)
        m.bm.faces.ensure_lookup_table()
        for f in list(m.bm.faces)[before:]:
            f.material_index = 1
            for lp in f.loops:
                u, v = lp[m.uv].uv
                lp[m.uv].uv = (tx / ATLAS + ((u * ATLAS - tx) % 40 + 2) / ATLAS,
                               ty / ATLAS + ((v * ATLAS - ty) % 40 + 2) / ATLAS)

    g = GATE_HALF
    hinge, shut = -g + 0.06, g - 0.05
    if lod == 2:
        # About 8 px wide at 400 m: one thin panel over the leaf's outline.
        beam(Vector((hinge, 0, 0.62)), Vector((shut, 0, 0.62)), 0.04, 1.1)
        return
    n = 3 if lod == 0 else 1
    beam(Vector((hinge, 0, 0.1)), Vector((hinge, 0, 1.17)), 0.1, 0.11, n)
    beam(Vector((shut, 0, 0.1)), Vector((shut, 0, 1.12)), 0.08, 0.09, n)
    bars = (0.18, 0.4, 0.62, 0.84) if lod == 0 else (0.29, 0.73)
    for z in bars:
        beam(Vector((hinge, 0, z)), Vector((shut, 0, z)), 0.04, 0.09, n)
    beam(Vector((hinge, 0, 1.08)), Vector((shut, 0, 1.08)), 0.065, 0.11, n)
    beam(Vector((hinge + 0.04, 0, 0.2)), Vector((0.1, 0, 1.04)), 0.04, 0.08, n)


GATE_HALF, POST_X = 1.29, 1.5


def build(piece: str, out: pathlib.Path, generator: str, *, kind: str, arms: tuple[float, ...] = (),
          texel: float = 270.0) -> None:
    """The whole wall piece: geometry for three LODs, numpy maps, collider, export and reports."""
    assert kind in ("run", "corner", "gate"), kind
    set_texel(texel)
    scene.reset_scene(piece)
    stone = materials.pbr_material(f"{piece}_stone", materials.srgb(GRANITE_HI), 0.88)
    timber = materials.pbr_material(f"{piece}_timber", materials.srgb("#4d4438"), 0.85) if kind == "gate" else None
    if kind == "gate":
        # Two short stubs from the posts out to the piece's ends: arms at the far ends, square inside.
        frames = [{"a": Vector((POST_X - 0.1, 0, 0)), "d": Vector((1, 0, 0)), "len": ARM - POST_X + 0.1,
                   "k": {1: None, -1: None}, "far_end": True},
                  {"a": Vector((-POST_X + 0.1, 0, 0)), "d": Vector((-1, 0, 0)), "len": ARM - POST_X + 0.1,
                   "k": {1: None, -1: None}, "far_end": True}]
    else:
        frames = arm_frames(kind, arms)
    centre = kind == "corner" and len(arms) > 1
    # Atlas layout: every side face's chunks (from the LOD0 rows' extent), drawn from one image.
    at = Atlas()
    faces_px = {}
    for ai, fr in enumerate(frames):
        for side in (1, -1):
            k = fr["k"][side]
            t_lo = min((k or 0.0) * half_w(z) for z in face_rows(0))
            ln = fr["len"]
            # One continuous image per face, 4 cm longer than the face so rounding never runs off it.
            l_img = ln - t_lo + 0.04
            img = stone_face(f"{piece}:{ai}:{side}", l_img)
            total = img["albedo"].shape[1]
            chunks, t0 = [], t_lo
            while t0 < ln - 1e-6:
                x0, y0, got = at.strip(ln - t0)
                t1 = ln if ln - t0 - got < 1e-6 else t0 + got
                w = int(math.ceil((t1 - t0) * TEXEL))
                if side == -1:
                    # The right face runs u against t (u = 0 at t1): flip the image so neighbouring
                    # chunks still meet.
                    flipped = {k2: v[:, ::-1] for k2, v in img.items()}
                    src = total - GUT - int(round((t1 - t_lo) * TEXEL))
                    at.put(flipped, x0, y0, src, w, 0.3)
                else:
                    at.put(img, x0, y0, GUT + int(round((t0 - t_lo) * TEXEL)), w, 0.3)
                chunks.append({"t0": t0, "t1": t1, "x0": x0, "y0": y0})
                t0 = t1
            faces_px[(ai, side)] = chunks
    for i in range(4):
        tile = slab_tile(f"{piece}:slab:{i}", *COPE_TILE)
        x0 = i * COPE_PX[0] + GUT
        at.put(tile, x0, COPE_Y0 + GUT, GUT, int(COPE_TILE[0] * TEXEL), 0.75)
    end = stone_face(f"{piece}:end", END_M[0], big=True)
    end_img = {k2: v[: int(END_M[1] * TEXEL) + 2 * GUT] if v.shape[0] >= int(END_M[1] * TEXEL) + 2 * GUT else
               np.pad(v, ((0, int(END_M[1] * TEXEL) + 2 * GUT - v.shape[0]), (0, 0)) + ((0, 0),) * (v.ndim - 2),
                      mode="edge") for k2, v in end.items()}
    at.put(end_img, END_REGION[0] + GUT, END_REGION[1] + GUT, GUT, int(END_M[0] * TEXEL), 0.4)
    if kind == "gate":
        pimg = slab_tile(f"{piece}:post", POST_M[0], POST_M[1])
        at.put(pimg, POST_REGION[0] + GUT, POST_REGION[1] + GUT, GUT, int(POST_M[0] * TEXEL), 0.6)

    lod_objs = []
    for lod in range(3):
        m = Mesh()
        build_body(m, piece, frames, lod, at, faces_px, end_square=kind != "gate")
        for s in cope_slabs(piece, frames, lod, centre):
            tile = random.Random(f"{piece}:{s['c'].x:.3f}:{s['c'].y:.3f}").randrange(4)
            box(m, s["c"], s["ax"], s["half"], (tile * COPE_PX[0] + GUT, COPE_Y0 + GUT))
        if kind == "gate":
            for x in (-POST_X, POST_X):
                post(m, Vector((x, 0, 0)), lod)
            gate(m, lod)
        mats = [stone] + ([timber] if timber else [])
        lod_objs.append(m.object(f"{piece}_lod{lod}", mats))
    tris = [geom.tri_count(o) for o in lod_objs]
    stone_only = lod_objs[0].copy()
    stone_only.data = lod_objs[0].data.copy()
    scene.link(stone_only)
    bm = bmesh.new()
    bm.from_mesh(stone_only.data)
    bmesh.ops.delete(bm, geom=[f for f in bm.faces if f.material_index != 0], context="FACES")
    bm.to_mesh(stone_only.data)
    bm.free()
    texel = {stone.name: uv.measure_texel_density(stone_only, ATLAS)}
    bpy.data.objects.remove(stone_only, do_unlink=True)
    zs = [v.co.z for v in lod_objs[0].data.vertices]
    print(f"{piece}: tris per LOD {tris}, texel {texel}, z {min(zs):.3f}..{max(zs):.3f}, free strips {at.free}")
    budgets.check(tuple(tris), "prop", piece)
    assert texel[stone.name]["median"] >= 256, texel
    assert WALL_H - 0.06 <= max(zs) <= WALL_H + 0.12 or kind == "gate", max(zs)

    # Maps: albedo 1024 (sRGB), normal and ORM box-filtered to 512, masks 256.
    gy, gx = np.gradient(at.hgt * TEXEL)
    n = np.stack([-gx, -gy, np.ones_like(gx)], -1)
    n /= np.linalg.norm(n, axis=-1, keepdims=True)
    half = NORMAL_SIZE
    n2 = n.reshape(half, 2, half, 2, 3).mean(axis=(1, 3))
    n2 /= np.linalg.norm(n2, axis=-1, keepdims=True)
    normal = np.concatenate([n2 * 0.5 + 0.5, np.ones((half, half, 1))], -1).astype(np.float32)
    orm = np.stack([at.ao, at.rough, np.zeros_like(at.ao)], -1).reshape(half, 2, half, 2, 3).mean(axis=(1, 3))
    orm = np.concatenate([orm, np.ones((half, half, 1))], -1).astype(np.float32)
    alb = np.concatenate([at.alb, np.ones((ATLAS, ATLAS, 1), np.float32)], -1)
    bake.write_png(ATLAS, alb.reshape(-1, 4), str(out / f"{piece}.albedo.png"), non_color=False)
    bake.write_png(half, normal.reshape(-1, 4), str(out / f"{piece}.normal.png"))
    bake.write_png(half, orm.reshape(-1, 4), str(out / f"{piece}.orm.png"))
    # masks (linear): R snow capacity (faces 0.3, cope and tops 0.75), G porosity 0.55, B 0, A = 1
    # (the joint AO is in ORM.R); the timber patch R 0.4, G 0.6.
    f = ATLAS // MASK_SIZE
    snow = at.snow.reshape(MASK_SIZE, f, MASK_SIZE, f).mean(axis=(1, 3))
    mk = np.stack([snow, np.full_like(snow, 0.55), np.zeros_like(snow), np.ones_like(snow)], -1).astype(np.float32)
    tx, ty, ts = TIMBER_REGION
    mk[(ty - GUT) // f:(ty + ts + GUT) // f, (tx - GUT) // f:(tx + ts + GUT) // f] = (0.4, 0.6, 0.0, 1.0)
    bake.write_png(MASK_SIZE, mk.reshape(-1, 4), str(out / f"{piece}.masks.png"))

    # Collider: one box per arm (the gateway: one box over the whole 4 m, gate shut).
    cbm = bmesh.new()
    boxes = ([(Vector((-ARM, 0, 0)), Vector((1, 0, 0)), 2 * ARM)] if kind in ("run", "gate") else
             [(fr["a"], fr["d"], fr["len"]) for fr in frames])
    for a, d, ln in boxes:
        perp = Vector((-d.y, d.x, 0))
        t0 = 0.0 if kind in ("run", "gate") else -FOOT_HW * 0.5
        vs = [cbm.verts.new(a + d * t + perp * s * FOOT_HW + Vector((0, 0, z)))
              for z in (0.0, WALL_H) for t, s in ((t0, -1), (ln, -1), (ln, 1), (t0, 1))]
        for q in ((0, 3, 2, 1), (4, 5, 6, 7), (0, 1, 5, 4), (1, 2, 6, 5), (2, 3, 7, 6), (3, 0, 4, 7)):
            cbm.faces.new([vs[i] for i in q])
    cme = bpy.data.meshes.new("collider_src")
    cbm.to_mesh(cme)
    cbm.free()
    csrc = scene.link(bpy.data.objects.new("collider_src", cme))
    col = collider.collider_from(csrc, "cap", f"{piece}_collider")
    bpy.data.objects.remove(csrc, do_unlink=True)
    bpy.context.view_layer.update()

    root = scene.empty(piece)
    for o in lod_objs:
        o.parent = root
    col.parent = root
    export.export_glb(root, str(out / f"{piece}.glb"))
    export.write_export_report(str(out / "export-report.json"), root, piece, generator)
    bake.write_bake_report(str(out / "bake-report.json"), receiverSignature=bake.receiver_signature([lod_objs[0]]),
                           device="none (numpy-drawn stone faces, no Cycles bake)", samples=0, marginPx=GUT,
                           channels={"albedo": {"size": ATLAS, "colorspace": "srgb"},
                                     "normal": {"size": NORMAL_SIZE, "colorspace": "linear"},
                                     "orm": {"size": NORMAL_SIZE, "colorspace": "linear"},
                                     "masks": {"size": MASK_SIZE, "colorspace": "linear"}},
                           texelDensityPxPerM=texel, extentZ=[min(zs), max(zs)],
                           wall={"kind": kind, "arms": list(arms), "footHalfWidthM": FOOT_HW, "topHalfWidthM": TOP_HW,
                                 "bodyHeightM": BODY_H, "heightM": WALL_H, "buryM": BURY, "armM": ARM,
                                 "gateHalfM": GATE_HALF if kind == "gate" else None,
                                 "postX": POST_X if kind == "gate" else None},
                           skipped=os.environ.get("HEXW_SKIP_BAKE") == "1")
