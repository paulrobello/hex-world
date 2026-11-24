"""Waterfall fall faces, waterfall_l1, waterfall_l2 and waterfall_l3 (rivers-at-levels spec section 4,
plan Task 18; refs assets-source/refs/waterfall, owner approval 2026-11-22: ref-1 the design, ref-4 the
8 m drop's single ledge, ref-7 the 12 m drop's three tiers, ref-10 the eye-level layout, ref-2's wet
streaks and glossy wet stone only; WF-Q2 a natural granite lip, WF-Q3 a lip socket per tier, WF-Q4 a
14 m face).

The face covers the upper tile's cliff column at a waterfall seam, drawn under the upper tile's root on
its edge 0, so every position is in the upper tile's frame (Blender x out of the seam, y along it, z up;
runtime x, z, -y). It is one parametric surface over (s, d): s the offset along the seam (|s| <= 7),
z the height. Its perimeter lies on the border plane
x = APOTHEM: the top ring on the upper channel profile (`_river.profile_z` at `_river.OFFS`, the
river seam's own section, plus the side ends), the bottom ring on the lower tile's, `drop` levels
down, and the side ends at |s| = 7 flush with the column. Rock stands out of the plane inside that
perimeter:
- one tier per level of drop (RQ2 O2: two tiers at 8 m, three at 12 m) on the controller's tier
  contract (WF-Q3, 2026-11-22): tier i's lip at the upper water level less 4 m per tier, set out
  1.5 m per tier from the seam, under an 8 m sheet on the lane. Tier 0's lip is the upper river's
  own surface at the seam; every further tier's lip is the level nose of a granite ledge whose shelf
  catches the sheet above, and the face under each sheet stands back behind its lip;
- each further tier's ledge is a group of rounded granite slabs (ref-4, ref-7): a lip slab across the
  sheet span (two or three slabs butted on the lip line), its nose straight and level on the lip along
  the 8 m sheet, a second course of slabs stacked under it and set back behind the sheet, and one
  slab or a stack of two either side, each set lower or further back, with rounded, broken noses and
  undercuts;
- the rest is masonry (ref-1): courses of rounded granite blocks with joints, the course joints on
  each ledge's step and a vertical joint each side of the sheet span. Behind the sheets the blocks
  stand low and scoured; beside them they stand proud, and the stepped mass under the ledges gives
  way to the banks at |s| 4.6 to 6.2, which roll out under the top ring as a natural lip (no dressed
  coping, WF-Q2);
- fine relief on the high poly only, carried by the normal bake.
Under the last lip the face returns to the border plane over the 1.55 m between the lower water and
the lower bed.

Every LOD is the same surface on its own constrained Delaunay of (s, z), all sharing the one
perimeter, so no LOD opens a crack at the seam (a border piece is never decimated). UV0 is the
surface parameter (s along U, the centre profile's arc length down from each column's top ring along
V), so LOD1 and LOD2 sample LOD0's atlas. The bake is `_boulder`'s: `_rock.granite_material` with `_boulder.sparser_moss` and
`_outcrop.vary_lichen`, so the face is one family with the boulders and outcrops, plus a wet layer
(the spray zone behind the sheet, the pool, the ledge landings and ref-2's streaks) darkening the
albedo and lowering the roughness. Masks: R snow, G wetness near 1, B 0, A 1 (the AO is in ORM.R).
Collider: one box per tier. Sockets: socket-lip-<tier> per tier (spec section 4, Lip sockets)."""
from __future__ import annotations

import os
import pathlib
import random

import bmesh
import bpy
import numpy as np
from mathutils import Matrix, Vector, noise
from mathutils.geometry import delaunay_2d_cdt

from lib import bake, budgets, export, geom, hexgeo, materials, scene, uv

import _boulder
import _outcrop
import _river
import _rock

A = hexgeo.APOTHEM
HALF_W = 7.0  # WF-Q4: 14 m, centred on the lane
STEP = hexgeo.LEVEL_STEP
# The controller's tier contract (WF-Q3, 2026-11-22; hex_play::water::tiles::fall_tiers and
# LEDGE_STEP): tier i's lip at the upper water level less STEP * i, set out LEDGE_STEP * i from the
# seam, under an 8 m sheet centred on the lane.
WATER = _river.WATER
LEDGE_STEP = 1.5
SHEET_W = 8.0
PROUD = 0.06  # interior rock stands at least this far out (no z-fight with the column's soil)
REC = 0.55  # the masonry behind a ledge's sheet stands this far behind that ledge's lip
STEP_UNDER = 0.35  # the masonry steps out to the next sheet's face this far under each lip
FOOT_H = 1.55  # the face returns to the border plane over this height above the bottom ring
TONGUE = (4.6, 6.2)  # the stepped mass spans |s| <= 4.6 and gives way to the bank rock by 6.2
LIP_ROLL = 0.30  # the banks' natural lip (WF-Q2) rolls this far out under the top ring
# Masonry (ref-1): courses of rounded granite blocks with joints, the course joints on each ledge's
# step, a vertical joint at each side of the sheet span.
COURSE_H = 1.7  # mean course height (m)
BLOCK_W = (2.0, 3.2)  # block width along the seam (m)
SPAN_EDGE = (4.0, 4.3)  # the vertical joint each side of the sheet span (|s|, m)
JOINT = (0.03, 0.12)  # each block edge stands in from its course and neighbours by this (m)
JOINT_D = 0.10  # joints sink this far below the joint floor
JOINT_SPAN = (0.03, 0.25)  # behind the sheets: the course joints wander this much per block edge
ROUND = (0.22, 0.40)  # block edge radius (m)
ROUND_BANK, JOINT_BANK = (0.45, 0.75), (0.03, 0.35)  # beside the sheets: rounder blocks, irregular course joints
AMP_T0, AMP_SHEET, AMP_BANK = (0.05, 0.10), (0.10, 0.20), (0.35, 0.80)  # block front proud of the joints (m)
# Ledge slabs (ref-4, ref-7): per ledge a lip slab over the sheet span, straight along it, and a slab
# or a stack of two either side, each with a rounded nose, a short front and a rounded undercut.
NOSE = (0.22, 0.12)  # the lip slab's nose radius out (x) and up (z), m
SHELF = 0.20  # a slab's shelf rises this much from its nose back into the masonry
SHELF_RISE = NOSE[1] + SHELF  # the lip slab's shelf meets the masonry this far above the lip
FRONT = 0.30  # the lip slab's front below the lip (m)
THICK = 0.85  # the lip slab's thickness (m)
UNDER = 0.55  # its underside curls this far back under the lip (m)
LIP_JOINT = (0.02, 0.25, 0.16)  # a joint in the lip slab: depth at the nose, below it and on the shelf, half width (m)
SLAB_END = 0.35  # a slab's rounded end (m)
SAMPLES, MARGIN = 32, 8
ATLAS, NORMAL_SIZE, ORM_SIZE, MASK_SIZE = 1024, 512, 512, 256
CAGE, MAX_RAY, AO_DIST = 0.30, 0.60, 0.8
UV_MARGIN = 0.012
STRIP_GAP = 0.15  # m between UV strips
HIGH_STEP = (0.06, 0.05)  # high-poly grid along the face and down its arc (m)
EDGE_GAP = 0.12  # no interior vertex nearer the perimeter than this (parameter space, m)


def smoothstep(e0, e1, x):
    t = np.clip((np.asarray(x, float) - e0) / (e1 - e0), 0.0, 1.0)
    return t * t * (3.0 - 2.0 * t)


def top_z(s):
    """Upper channel profile at offset s along the seam (Blender z, relative to the upper level)."""
    return np.vectorize(_river.profile_z, otypes=[float])(np.asarray(s, float))


def reach_max(drop: int) -> float:
    """kit_gate::waterfall_reach_max less 5 cm: the last lip's set-out and 1 m of bank rock."""
    return LEDGE_STEP * (drop - 1) + 0.95


class Face:
    """The parametric fall face for a drop of `drop` levels (module docstring): a height field of
    x over (s, z), z the Blender height, on the domain between the two channel profiles."""

    def __init__(self, piece: str, drop: int):
        self.piece, self.drop = piece, drop
        self.F = drop * STEP
        self.rings = {}
        self.lip_z = [WATER - STEP * i for i in range(drop)]
        self.lip_x = [LEDGE_STEP * i for i in range(drop)]
        self.base = [PROUD] + [max(PROUD, o - REC) for o in self.lip_x[1:]]
        wz = np.linspace(2.0, -self.F - 6.0, 2001)
        self.wob_z = -wz
        self.wob = np.array([noise.noise(Vector((0.0, float(zz) * 0.45, 2.3))) for zz in wz])
        self.courses(random.Random(f"{piece}-blocks"))
        self.slabs = self.ledges(random.Random(f"{piece}-slabs"))
        # Arc length of the centre profile from z = 0.5 down, the V axis of UV0 and the high poly.
        z = np.linspace(0.5, -self.F - 3.6, 40001)
        x = self.surface(np.zeros_like(z), z)[0]
        self.arc_z = z
        self.arc = np.concatenate(([0.0], np.cumsum(np.hypot(np.diff(x), np.diff(z)))))
        s = np.linspace(-HALF_W, HALF_W, 281)
        t = top_z(s)
        self.H = float(np.max(self.A(t - self.F) - self.A(t)))
        # The tier contract holds along the whole sheet span: every ledge's nose on its lip line.
        ss = np.linspace(-SHEET_W / 2, SHEET_W / 2, 161)
        for i in range(1, drop):
            off = self.relief(ss, np.full(ss.shape, self.lip_z[i])) - self.lip_x[i]
            assert np.abs(off).max() <= 0.03, (piece, "lip line", i, float(np.abs(off).max()))

    # --- layout ----------------------------------------------------------------------------------
    def courses(self, rng):
        """The masonry: course joints (descending z) on each ledge's step and between, and per course
        its blocks, with a vertical joint each side of the sheet span. Rows of self.blk: s0, s1, z0,
        z1, edge radius, front, bulge, tilt along s, tilt down."""
        edges = [1.0]
        for lo in [L - STEP_UNDER for L in self.lip_z[1:]] + [-self.F - 3.3]:
            hi = edges[-1]
            w = np.array([rng.uniform(0.7, 1.3) for _ in range(max(1, int(round((hi - lo) / COURSE_H))))])
            edges += [float(c) for c in hi - (hi - lo) * np.cumsum(w) / w.sum()]
        self.course_z = np.array(edges)
        first_step = self.lip_z[1] - STEP_UNDER if self.drop > 1 else -np.inf
        rows, self.course_br = [], []
        for c in range(len(edges) - 1):
            zt, zb = edges[c], edges[c + 1]
            e = rng.uniform(*SPAN_EDGE)
            w = np.array([rng.uniform(0.7, 1.3) for _ in range(rng.choice((2, 3, 3)))])
            inner = (-e + 2 * e * np.cumsum(w)[:-1] / w.sum()).tolist()
            left, right = [-e], [e]
            while left[-1] > -9.5:
                left.append(left[-1] - rng.uniform(*BLOCK_W))
            while right[-1] < 9.5:
                right.append(right[-1] + rng.uniform(*BLOCK_W))
            br = left[::-1] + inner + right
            self.course_br.append((len(rows), np.array(br)))
            for j in range(len(br) - 1):
                sc = 0.5 * (br[j] + br[j + 1])
                bank = abs(sc) > e
                jz = JOINT_BANK if bank else JOINT_SPAN
                s0, s1 = br[j] + rng.uniform(*jz), br[j + 1] - rng.uniform(*jz)
                z0, z1 = zb + rng.uniform(*jz), zt - rng.uniform(*jz)
                r = min(rng.uniform(*(ROUND_BANK if bank else ROUND)), 0.45 * (s1 - s0), 0.45 * (z1 - z0))
                amp = AMP_BANK if bank else AMP_T0 if zb >= first_step - 1e-6 else AMP_SHEET
                ps, pz = np.meshgrid(np.linspace(s0 + 0.06, s1 - 0.06, 5), np.linspace(z0 + 0.06, z1 - 0.06, 5))
                front = float(self.env(ps.ravel(), pz.ravel()).max()) + rng.uniform(*amp)
                k = 0.25 if bank else 0.03
                rows.append((s0, s1, z0, z1, r, front, rng.uniform(0.3, 1.0) * k, rng.uniform(-k, k), rng.uniform(-k, k)))
        self.blk = np.array(rows)

    def ledges(self, rng):
        """Ledge slabs for tiers 1 on: the lip slab across the sheet span, straight on the lip along
        it, and either side a slab, lower or further back, sometimes with a second stacked under it."""
        slabs = []
        for i in range(1, self.drop):
            L, O = self.lip_z[i], self.lip_x[i]
            a, b = rng.uniform(4.4, 4.9), rng.uniform(4.4, 4.9)
            # The lip slab is two or three slabs butted on the lip line, the joints clear of the socket.
            joints = sorted(rng.choice((-1.0, 1.0)) * rng.uniform(0.9, 2.6) for _ in range(rng.choice((1, 2))))
            if len(joints) == 2 and (joints[1] - joints[0] < 1.6 or joints[0] * joints[1] > 0):
                joints = [-abs(joints[0]) - 0.2, abs(joints[1]) + 0.2]
            seg_t = [THICK * rng.uniform(0.85, 1.15) for _ in range(len(joints) + 1)]
            seg_u = [UNDER * rng.uniform(0.75, 1.15) for _ in range(len(joints) + 1)]
            seg_rz = [NOSE[1] * rng.uniform(0.8, 1.8) for _ in range(len(joints) + 1)]
            slabs.append(dict(s0=-a, s1=b, top=L, O=O, rx=NOSE[0], rz=NOSE[1], f=FRONT, T=THICK, U=UNDER,
                              re=SLAB_END, lip=True, joints=joints, seg_t=seg_t, seg_u=seg_u, seg_rz=seg_rz))
            # Stacked under it, set back behind the sheet: a second course of slabs of its own lengths.
            a2, b2 = rng.uniform(2.8, 4.6), rng.uniform(2.8, 4.6)
            under = []
            for _ in range(200):
                if len(under) == rng.choice((1, 2)):
                    break
                c = rng.uniform(-a2 + 0.8, b2 - 0.8)
                if all(abs(c - j) > 0.6 for j in joints + under):
                    under.append(c)
            under.sort()
            n = len(under) + 1
            slabs.append(dict(s0=-a2, s1=b2, top=L - max(seg_t) - 0.05, O=O - rng.uniform(0.15, 0.25),
                              rx=rng.uniform(0.15, 0.25), rz=rng.uniform(0.08, 0.14), f=rng.uniform(0.2, 0.3),
                              T=THICK, U=UNDER, re=rng.uniform(0.3, 0.45), lip=False, joints=under,
                              seg_t=[rng.uniform(0.6, 0.85) for _ in range(n)], seg_u=[rng.uniform(0.3, 0.45) for _ in range(n)],
                              seg_rz=[rng.uniform(0.08, 0.14) for _ in range(n)]))
            for side, inner in ((-1.0, a), (1.0, b)):
                w = rng.uniform(1.0, 1.6)
                top = L + rng.uniform(-0.4, 0.05)
                of = O * rng.uniform(0.72, 0.9)
                t = rng.uniform(0.6, 0.95)
                lo, hi = sorted((side * (inner + 0.04), side * (inner + 0.04 + w)))
                slabs.append(dict(s0=lo, s1=hi, top=top, O=of, rx=rng.uniform(0.18, 0.3), rz=rng.uniform(0.1, 0.18),
                                  f=rng.uniform(0.2, 0.35), T=t, U=rng.uniform(0.35, 0.55), re=rng.uniform(0.3, 0.45),
                                  lip=False))
                if rng.random() < 0.6:
                    sh = rng.uniform(0.15, 0.6)
                    w2 = rng.uniform(0.9, 1.4)
                    lo, hi = sorted((side * (inner + 0.04 + sh), side * (inner + 0.04 + sh + w2)))
                    slabs.append(dict(s0=lo, s1=hi, top=top - t - 0.05, O=of - rng.uniform(0.1, 0.3),
                                      rx=rng.uniform(0.18, 0.3), rz=rng.uniform(0.1, 0.18), f=rng.uniform(0.2, 0.35),
                                      T=rng.uniform(0.6, 0.9), U=rng.uniform(0.3, 0.5), re=rng.uniform(0.3, 0.45),
                                      lip=False))
        return slabs

    # --- the surface ---------------------------------------------------------------------------
    def mass(self, z):
        """The stepped mass under the ledges on the lane's centre line: out to each sheet's face
        just under that sheet's lip."""
        z = np.asarray(z, float)
        x = np.full(z.shape, self.base[0])
        for i in range(1, self.drop):
            zs = self.lip_z[i] - STEP_UNDER
            x = x + (self.base[i] - self.base[i - 1]) * (1.0 - smoothstep(zs - 0.03, zs + 0.03, z))
        return x

    def env(self, s, z):
        """The masonry's joint floor: the stepped mass across the sheet span, giving way at
        TONGUE (wandering with height) to the banks' rolled lip."""
        s, z = np.broadcast_arrays(np.asarray(s, float), np.asarray(z, float))
        depth = top_z(s) - z
        bank = PROUD + LIP_ROLL * (smoothstep(0.0, 0.35, depth) - 0.75 * smoothstep(0.35, 1.8, depth))
        wob = np.interp(-z, self.wob_z, self.wob)
        t0 = TONGUE[0] + 0.4 * wob
        tongue = 1.0 - smoothstep(t0, t0 + (TONGUE[1] - TONGUE[0]) * (1.0 + 0.4 * wob), np.abs(s))
        return np.maximum(bank, self.mass(z) * tongue)

    def masonry(self, s, z):
        """The block field over the joint floor: (x out of the border plane, joint weight 1 in a
        joint and 0 on a block's face)."""
        s, z = np.broadcast_arrays(np.asarray(s, float), np.asarray(z, float))
        shape = s.shape
        s, z = s.ravel(), z.ravel()
        c = np.clip(np.searchsorted(-self.course_z, -z, side="right") - 1, 0, len(self.course_z) - 2)
        k = np.zeros(len(s), int)
        for ci, (off, br) in enumerate(self.course_br):
            m = c == ci
            if m.any():
                k[m] = off + np.clip(np.searchsorted(br, s[m], side="right") - 1, 0, len(br) - 2)
        b = self.blk[k]
        cs, cz, hs, hz, r = (b[:, 0] + b[:, 1]) / 2, (b[:, 2] + b[:, 3]) / 2, (b[:, 1] - b[:, 0]) / 2, (b[:, 3] - b[:, 2]) / 2, b[:, 4]
        qs, qz = np.abs(s - cs) - (hs - r), np.abs(z - cz) - (hz - r)
        d = r - (np.hypot(np.maximum(qs, 0.0), np.maximum(qz, 0.0)) + np.minimum(np.maximum(qs, qz), 0.0))
        prof = np.sqrt(1.0 - (1.0 - np.clip(d / r, 0.0, 1.0)) ** 2)
        u, v = (s - cs) / hs, (z - cz) / hz
        lump = np.array([noise.noise(Vector((a * 0.55, c * 0.55, 4.4 + kk * 0.37))) for a, c, kk in zip(s, z, k)])
        front = b[:, 5] + b[:, 6] * (1.0 - 0.5 * np.clip(u * u + v * v, 0.0, 2.0) + 1.2 * lump) + b[:, 7] * u + b[:, 8] * v
        g = self.env(s, z) - JOINT_D
        x = g + (np.maximum(front, g) - g) * prof
        joint = 1.0 - smoothstep(0.0, 0.7, d / r)
        return x.reshape(shape), joint.reshape(shape)

    def slab(self, sl, s, z):
        """One ledge slab's x (negative where it is not): shelf, rounded nose, front, rounded
        undercut, rounded ends."""
        L, O, rx, rz, f, T, U, re = (sl[k] for k in ("top", "O", "rx", "rz", "f", "T", "U", "re"))
        if "joints" in sl:
            seg = np.searchsorted(sl["joints"], s)
            T, U, rz = np.asarray(sl["seg_t"])[seg], np.asarray(sl["seg_u"])[seg], np.asarray(sl["seg_rz"])[seg]
        x = (O - rx) * (1.0 - (z - (L + rz)) / SHELF)
        t = np.clip((z - L) / rz, 0.0, 1.0)
        x = np.where((z >= L) & (z < L + rz), O - rx + rx * np.sqrt(1.0 - t * t), x)
        x = np.where((z < L) & (z >= L - f), O - 0.04 * (L - z) / f, x)
        t = np.clip((L - f - z) / (T - f), 0.0, 1.0)
        x = np.where((z < L - f) & (z >= L - T), O - 0.04 - U + U * np.sqrt(1.0 - t * t), x)
        x = np.where(z < L - T, O - 0.04 - U - 6.0 * (L - T - z), x)
        e = np.clip(np.maximum(s - (sl["s1"] - re), (sl["s0"] + re) - s) / re, 0.0, None)
        x = np.where(e >= 1.0, -1.0, x * np.sqrt(np.clip(1.0 - e * e, 0.0, 1.0)))
        if "joints" in sl:
            d0, d1, jw = LIP_JOINT
            depth = d0 + (d1 - d0) * np.maximum(smoothstep(0.05, 0.15, L - z), smoothstep(L + rz, L + rz + 0.06, z))
            for sj in sl["joints"]:
                x = x - depth * (1.0 - smoothstep(0.0, jw, np.abs(s - sj)))
        # Broken noses and fronts, except the lip slab's nose along the sheet span.
        w = np.ones(x.shape) if not sl["lip"] else np.maximum(smoothstep(4.0, 4.4, np.abs(s)), smoothstep(0.03, 0.15, L - z))
        on = (x > 0.0) & (w > 0.0)
        if on.any():
            n = np.array([noise.noise(Vector((a * 1.3, b * 1.3, 9.1))) for a, b in zip(s[on], z[on])])
            x[on] += 0.07 * w[on] * n
        return x

    def surface(self, s, z):
        """Masonry and ledge slabs before the perimeter's fades: (x, moss weight in the joints)."""
        s, z = np.broadcast_arrays(np.asarray(s, float), np.asarray(z, float))
        xm, joint = self.masonry(s, z)
        x, crease = xm.copy(), np.zeros(s.shape)
        for sl in self.slabs:
            xs = self.slab(sl, s, z)
            crease = np.maximum(crease, (1.0 - smoothstep(0.0, 0.15, np.abs(xs - x))) * (xs > 0.05))
            x = np.maximum(x, xs)
        return x, np.maximum(joint * (xm >= x - 1e-9), crease)

    def relief(self, s, z, moss=False):
        """Offset out of the border plane (m) at (s, z): exactly 0 on the perimeter."""
        s, z = np.broadcast_arrays(np.asarray(s, float), np.asarray(z, float))
        tz = top_z(s)
        depth = tz - z
        side = smoothstep(0.0, 1.0, HALF_W - np.abs(s))
        foot = smoothstep(0.0, FOOT_H, z - (tz - self.F))
        top = smoothstep(0.0, 0.4, depth)
        x, mo = self.surface(s, z)
        dmin = np.clip(np.minimum(np.minimum(depth, z - (tz - self.F)), HALF_W - np.abs(s)), 0.0, None)
        x = np.maximum(side * foot * top * x, PROUD * np.minimum(1.0, dmin / 0.2))
        # Soft ceiling at the reach: a block nearing it rounds off rather than flattening.
        r, k = reach_max(self.drop), 0.25
        x = np.where(x > r - k, r - k * np.exp(-(x - (r - k)) / k), x)
        return (x, mo) if moss else x

    def point(self, s, z, fine=None):
        """Blender (x, y, z) of the face at (s, z)."""
        x = A + self.relief(s, z)
        if fine is not None:
            x = x + fine
        return np.stack(np.broadcast_arrays(x, np.asarray(s, float), np.asarray(z, float)), -1)

    def moss(self, s, z):
        """Moss weight: the joints and the creases behind the slabs, broken up, sparse behind the
        sheets."""
        s, z = np.asarray(s, float).ravel(), np.asarray(z, float).ravel()
        mo = self.relief(s, z, moss=True)[1]
        n = np.array([noise.noise(Vector((a * 0.9 + 7.7, b * 0.9, 3.3))) for a, b in zip(s, z)])
        behind = 1.0 - smoothstep(3.6, 4.8, np.abs(s))
        return np.clip(mo * smoothstep(-0.2, 0.3, n) * (1.0 - 0.7 * behind), 0.0, 1.0)

    def wet(self, s, z):
        """Wet weight: behind the sheets, the pool's spray, each ledge's shelf and ref-2's streaks."""
        s, z = np.asarray(s, float).ravel(), np.asarray(z, float).ravel()
        behind = 1.0 - smoothstep(3.4, 4.2, np.abs(s))
        spray = 0.75 * (1.0 - smoothstep(0.0, 3.0, z - (WATER - self.F)))
        land = np.zeros_like(behind)
        for L in self.lip_z[1:]:
            land = np.maximum(land, 0.6 * (1.0 - smoothstep(0.0, 1.4, np.abs(z - L))))
        streak = np.array([noise.noise(Vector((a * 1.7 + 3.1, b * 0.21 - 1.3, 5.7))) for a, b in zip(s, z)])
        streak = 0.8 * smoothstep(0.12, 0.38, streak) * (1.0 - smoothstep(6.0, 6.9, np.abs(s)))
        return np.clip(np.maximum.reduce([behind, spray, land, streak]), 0.0, 1.0)

    # --- parameter-space layout ----------------------------------------------------------------
    def A(self, z):
        """Centre-profile arc length from z = 0.5 down to z."""
        return np.interp(-np.asarray(z, float), -self.arc_z, self.arc)

    def z_of(self, a):
        return np.interp(a, self.arc, self.arc_z)

    def V(self, s, z):
        """UV0's V in metres: arc length down the centre profile from this column's top ring."""
        return self.A(z) - self.A(top_z(s))

    def ring_s(self):
        return sorted({round(o, 9) for o in _river.OFFS} | {-HALF_W, HALF_W})

    def perimeter(self):
        """Perimeter (s, z) loop: top ring, right side down, bottom ring back, left side up."""
        rs = self.ring_s()
        side = [-float(v) for v in np.arange(1.0, self.F - 0.5, 1.0)]
        loop = [(s, float(top_z(s))) for s in rs]
        loop += [(HALF_W, z) for z in side]
        loop += [(s, float(top_z(s)) - self.F) for s in reversed(rs)]
        loop += [(-HALF_W, z) for z in reversed(side)]
        return loop

    def interior(self, h, a, level=0):
        """Interior (s, z) points: columns h apart (one on the lane's centre line), rows a apart down
        the centre profile's arc, and rows along the slabs (LOD0: every slab's shelf, nose, front
        and undercut and every course joint; LOD1: the course joints and the lip slabs'; LOD2: the
        lip slabs' shelf, lip, front and underside), none within EDGE_GAP of the perimeter."""
        half = np.arange(h, HALF_W, h)
        ss = np.concatenate((-half[::-1], [0.0], half))
        a0, a1 = float(self.A(0.0)), float(self.A(top_z(0.0) - self.F))
        n = max(1, int(round((a1 - a0) / a)))
        rows = [(float(z), -HALF_W, HALF_W) for z in self.z_of(np.linspace(a0, a1, n + 1))]
        extra = []
        if level < 2:
            rows += [(float(z), -HALF_W, HALF_W) for z in self.course_z]
        for sl in self.slabs:
            L, rz, f, T = sl["top"], max(sl.get("seg_rz", [sl["rz"]])), sl["f"], sl["T"]
            if level == 0 or (level == 1 and sl["lip"]):
                zs = (L + rz + SHELF, L + rz + 0.4 * SHELF, L + rz, L + 0.5 * rz, L, L - f, L - 0.5 * (f + T), L - T)
            elif sl["lip"]:
                zs = (L + rz + SHELF, L, L - f, L - T)
            else:
                continue
            rows += [(float(z), sl["s0"], sl["s1"]) for z in zs]
            if level < 2:
                jw = LIP_JOINT[2]
                extra += [(float(s), float(z)) for sj in sl.get("joints", ()) for s in (sj - jw, sj - 0.5 * jw, sj, sj + 0.5 * jw, sj + jw)
                          for z in zs]
        pts, seen = [], set()
        for z, lo, hi in rows:
            for s in ss[(ss >= lo - 1e-9) & (ss <= hi + 1e-9)]:
                tz = float(top_z(s))
                if min(tz - z, z - (tz - self.F), HALF_W - abs(s)) < EDGE_GAP:
                    continue
                k = (round(float(s), 4), round(float(z), 4))
                if k not in seen:
                    seen.add(k)
                    pts.append((float(s), float(z)))
        for s, z in extra:
            k = (round(s, 4), round(z, 4))
            if k not in seen:
                seen.add(k)
                pts.append((s, z))
        return pts

    def strips(self):
        """UV strips: two half-width strips stacked for the 4 m face (squarer, so denser), one for the
        taller faces. Sets uv_k (UV units per metre)."""
        self.n_strips = 2 if self.drop == 1 else 1
        w = 2 * HALF_W / self.n_strips
        self.strip_s0 = [-HALF_W + i * w for i in range(self.n_strips)]
        tall = self.n_strips * self.H + (self.n_strips - 1) * STRIP_GAP
        self.uv_k = (1.0 - 2 * UV_MARGIN) / max(w, tall)

    def strip_of(self, s):
        return min(self.n_strips - 1, int((s + HALF_W) // (2 * HALF_W / self.n_strips)))

    def uv_of(self, s, z, strip):
        """UV0 of (s, z) in UV strip `strip`."""
        k = self.uv_k
        v = float(self.V(s, z))
        return (UV_MARGIN + (s - self.strip_s0[strip]) * k, UV_MARGIN + (strip * (self.H + STRIP_GAP) + v) * k)

    # --- meshes --------------------------------------------------------------------------------
    def lod(self, name, h, a, level=0):
        loop = self.perimeter()
        inner = self.interior(h, a, level)
        coords = [Vector(p) for p in loop + inner]
        out_v, _e, out_f, orig, _oe, _of = delaunay_2d_cdt(coords, [], [list(range(len(loop)))], 1, 1e-6)
        # mathutils is single precision: snap every output vertex back to its exact input point.
        exact = loop + inner
        assert all(len(o) for o in orig), f"{name}: the CDT added a vertex"
        sz = np.array([exact[o[0]] for o in orig], float)
        ring = np.array([o[0] < len(loop) for o in orig])
        pts = self.point(sz[:, 0], sz[:, 1])
        pts[ring, 0] = A
        faces = []
        for f in out_f:
            a2, b2, c2 = (sz[i] for i in f[:3])
            area = (b2[0] - a2[0]) * (c2[1] - a2[1]) - (c2[0] - a2[0]) * (b2[1] - a2[1])
            faces.append(list(f) if area > 0 else list(reversed(f)))
        me = bpy.data.meshes.new(name)
        me.from_pydata([tuple(map(float, p)) for p in pts], [], faces)
        me.update()
        obj = scene.link(bpy.data.objects.new(name, me))
        for p in obj.data.polygons:
            p.use_smooth = True
        uvl = obj.data.uv_layers.new(name="UVMap")
        for poly in obj.data.polygons:
            st = self.strip_of(float(np.mean([sz[v][0] for v in poly.vertices])))
            for li, vi in zip(poly.loop_indices, poly.vertices):
                uvl.data[li].uv = self.uv_of(float(sz[vi][0]), float(sz[vi][1]), st)
        self.rings[name] = ring
        return obj, sz

    def high(self, name):
        hs, hv = HIGH_STEP
        ss = np.arange(-HALF_W - 0.3, HALF_W + 0.3 + 1e-9, hs)
        vs = np.arange(-0.3, self.H + 0.3 + 1e-9, hv)
        S, Vv = np.meshgrid(ss, vs)
        Z = self.z_of(self.A(top_z(S)) + Vv)
        fine = np.array([0.035 * noise.noise(Vector((a * 3.1, b * 3.1, 1.7))) + 0.012 * noise.noise(Vector((a * 11.0, b * 11.0, 7.3)))
                         for a, b in zip(S.ravel(), Z.ravel())]).reshape(S.shape)
        tz = top_z(S)
        dmin = np.clip(np.minimum(np.minimum(tz - Z, Z - (tz - self.F)), HALF_W - np.abs(S)), 0.0, None)
        fine = fine * np.minimum(1.0, dmin / 0.3)
        P = self.point(S, Z, fine)
        ny, nx = S.shape
        faces = []
        for j in range(ny - 1):
            for i in range(nx - 1):
                a0 = j * nx + i
                faces += [[a0, a0 + nx, a0 + 1], [a0 + 1, a0 + nx, a0 + nx + 1]]
        me = bpy.data.meshes.new(name)
        me.from_pydata([tuple(map(float, p)) for p in P.reshape(-1, 3)], [], faces)
        me.update()
        obj = scene.link(bpy.data.objects.new(name, me))
        for p in obj.data.polygons:
            p.use_smooth = True
        wet = self.wet(S, Z).astype(np.float32)
        obj.data.attributes.new("wet", "FLOAT", "POINT").data.foreach_set("value", wet)
        moss = self.moss(S, Z).astype(np.float32)
        obj.data.attributes.new("moss", "FLOAT", "POINT").data.foreach_set("value", moss)
        return obj

    def occluders(self, mat):
        """The lower channel's bed out from the foot and the column's soil beyond the side ends, for
        the AO bake only."""
        ss = np.linspace(-9.0, 9.0, 37)
        xs = np.linspace(A, A + 7.0, 15)
        verts = [(float(x), float(s), float(top_z(s)) - self.F) for x in xs for s in ss]
        faces = [[i * 37 + j, i * 37 + j + 1, (i + 1) * 37 + j + 1, (i + 1) * 37 + j] for i in range(14) for j in range(36)]
        for sign in (-1.0, 1.0):
            base = len(verts)
            for zc in np.linspace(0.0, -self.F - 1.0, 9):
                for sv in np.linspace(HALF_W, HALF_W + 3.0, 7):
                    verts.append((A, float(sign * sv), float(zc)))
            faces += [[base + i * 7 + j, base + i * 7 + j + 1, base + (i + 1) * 7 + j + 1, base + (i + 1) * 7 + j]
                      for i in range(8) for j in range(6)]
        me = bpy.data.meshes.new("waterfall_occluders")
        me.from_pydata(verts, [], faces)
        o = scene.link(bpy.data.objects.new("waterfall_occluders", me))
        o.data.materials.append(mat)
        return [o]


def wet_material(name: str) -> bpy.types.Material:
    """The boulders' granite (`_rock.granite_material`, `_boulder.sparser_moss`,
    `_outcrop.vary_lichen`) with the wet layer from the high poly's "wet" point attribute (a darker
    albedo and a low roughness) and moss in the joints from its "moss" attribute."""
    m = _rock.granite_material(name, soil_z=-1.0)
    _boulder.sparser_moss(m, _boulder.MOSS_CUT)
    _outcrop.vary_lichen(m)
    nt = m.node_tree
    N, L = nt.nodes, nt.links
    bsdf = next(n for n in N if n.type == "BSDF_PRINCIPLED")
    attr = N.new("ShaderNodeAttribute")
    attr.attribute_name = "wet"
    wet = attr.outputs["Fac"]

    def scaled(k):
        n = N.new("ShaderNodeMath")
        n.operation = "MULTIPLY"
        L.new(wet, n.inputs[0])
        n.inputs[1].default_value = k
        return n.outputs[0]

    col = bsdf.inputs["Base Color"].links[0].from_socket
    mix = N.new("ShaderNodeMix")
    mix.data_type = "RGBA"
    mix.blend_type = "MULTIPLY"
    ins = {s.identifier: s for s in mix.inputs}
    L.new(scaled(0.7), ins["Factor_Float"])
    L.new(col, ins["A_Color"])
    ins["B_Color"].default_value = (0.6, 0.6, 0.58, 1.0)
    mattr = N.new("ShaderNodeAttribute")
    mattr.attribute_name = "moss"
    mm = N.new("ShaderNodeMix")
    mm.data_type = "RGBA"
    mi = {s.identifier: s for s in mm.inputs}
    L.new(mattr.outputs["Fac"], mi["Factor_Float"])
    L.new(next(s for s in mix.outputs if s.identifier == "Result_Color"), mi["A_Color"])
    mi["B_Color"].default_value = (*materials.srgb("#55702a")[:3], 1.0)
    L.new(next(s for s in mm.outputs if s.identifier == "Result_Color"), bsdf.inputs["Base Color"])
    rough = bsdf.inputs["Roughness"].links[0].from_socket
    lerp = N.new("ShaderNodeMix")
    lerp.data_type = "FLOAT"
    li = {s.identifier: s for s in lerp.inputs}
    L.new(scaled(0.8), li["Factor_Float"])
    L.new(rough, li["A_Float"])
    li["B_Float"].default_value = 0.30
    L.new(next(s for s in lerp.outputs if s.identifier == "Result_Float"), bsdf.inputs["Roughness"])
    return m


def tier_boxes(face: Face, lod0, name):
    """One box per tier (plan Task 18): the tier's rock, from the shelf behind its lip (the top
    ring for tier 0) down to the next tier's shelf (the bottom ring for the last), from the border
    plane out to its furthest vertex, the whole 14 m wide."""
    co = np.array([v.co[:] for v in lod0.data.vertices])
    bands = [float(co[:, 2].max())] + [z + SHELF_RISE for z in face.lip_z[1:]] + [float(co[:, 2].min())]
    bm = bmesh.new()
    for k in range(face.drop):
        sel = (co[:, 2] <= bands[k] + 1e-6) & (co[:, 2] >= bands[k + 1] - 1e-6)
        x1 = float(co[sel, 0].max())
        z0, z1 = bands[k + 1], bands[k]
        res = bmesh.ops.create_cube(bm, size=1.0)
        for v in res["verts"]:
            v.co.x = (A + x1) / 2 + v.co.x * (x1 - A)
            v.co.y = v.co.y * 2 * HALF_W
            v.co.z = (z0 + z1) / 2 + v.co.z * (z1 - z0)
    bmesh.ops.triangulate(bm, faces=bm.faces)
    me = bpy.data.meshes.new(name)
    bm.to_mesh(me)
    bm.free()
    obj = scene.link(bpy.data.objects.new(name, me))
    obj["collider"] = "box"
    return obj


def check(face: Face, lods, szs):
    """The Task 17 gate's checks on the built LODs (kit_gate::waterfall_face_errors), so a bad build
    fails here: one perimeter, on the plane and the profiles, the interior proud and in reach, and
    every ledge's lip vertex where the tier contract puts it."""
    rings = []
    n_loop = len(face.perimeter())
    for o, sz in zip(lods, szs):
        co = np.array([v.co[:] for v in o.data.vertices])
        ring = face.rings[o.name]
        assert ring.sum() == n_loop, (o.name, int(ring.sum()), n_loop)
        rc = co[ring]
        assert np.all(np.abs(rc[:, 0] - A) < 1e-6), o.name
        zt = top_z(rc[:, 1])
        ok = (np.abs(rc[:, 2] - zt) < 1e-4) | (np.abs(rc[:, 2] - (zt - face.F)) < 1e-4) | (np.abs(np.abs(rc[:, 1]) - HALF_W) < 1e-6)
        assert ok.all(), (o.name, rc[~ok][:5])
        inner = co[~ring]
        lo, hi = inner[:, 0].min() - A, inner[:, 0].max() - A
        assert lo >= 0.01 and hi <= reach_max(face.drop) + 1e-6, (o.name, lo, hi)
        for i in range(1, face.drop):
            want = np.array((A + face.lip_x[i], 0.0, face.lip_z[i]))
            assert np.min(np.linalg.norm(co - want, axis=1)) < 0.01, (o.name, "lip", i)
        rings.append(sorted(map(tuple, np.round(rc, 5))))
    assert all(r == rings[0] for r in rings), "perimeter differs across LODs"


def size_lods(face: Face):
    """(h, a, tris) per LOD: interior spacing grown until LOD0 sits under the 4k ceiling and LOD1 and
    LOD2 under 50% and 15% of it."""
    specs, (h, a) = [], (0.22, 0.19)
    for level, ratio in enumerate((None, budgets.LOD1_MAX_RATIO, budgets.LOD2_MAX_RATIO)):
        t = 0
        for _ in range(120):
            o, _sd = face.lod("probe", h, a, level)
            t = geom.tri_count(o)
            me = o.data
            bpy.data.objects.remove(o, do_unlink=True)
            bpy.data.meshes.remove(me)
            limit = budgets.WATERFALL_LOD0_MAX - 60 if ratio is None else ratio * specs[0][2]
            if t <= limit:
                break
            h, a = h * 1.05, a * 1.05
        else:
            raise AssertionError(f"{face.piece}: no spacing meets LOD budget ratio {ratio} (last {t} tris)")
        specs.append((round(h, 4), round(a, 4), t))
    return specs


def build(piece: str, drop: int, out: pathlib.Path, generator: str, *, texel_min: float) -> None:
    skip_bake = os.environ.get("HEXW_SKIP_BAKE") == "1"
    scene.reset_scene(piece)
    face = Face(piece, drop)
    face.strips()
    mat_name = f"{piece}_rock"
    lod_mat = materials.pbr_material(mat_name, materials.srgb("#85827b"), 0.75)
    specs = size_lods(face)
    lods, szs = [], []
    for i, (h, a, _) in enumerate(specs):
        o, sz = face.lod(f"{piece}_lod{i}", h, a, i)
        o.data.materials.append(lod_mat)
        lods.append(o)
        szs.append(sz)
    tris = [geom.tri_count(o) for o in lods]
    texel = uv.measure_texel_density(lods[0], ATLAS)
    reach = max(v.co.x for v in lods[0].data.vertices) - A
    print(f"{piece}: tris per LOD {tris}, specs {specs}, texel {texel}, arc {face.H:.3f} m, uv_k {face.uv_k:.4f}, reach {reach:.3f}")
    check(face, lods, szs)
    budgets.check(tuple(tris), "prop", piece)
    assert texel["median"] >= texel_min, texel

    n_px = ATLAS * ATLAS
    albedo = np.zeros((n_px, 4), np.float32)
    albedo[:] = (*materials.srgb("#85827b")[:3], 1.0)
    normal = np.zeros((n_px, 4), np.float32)
    normal[:] = (0.5, 0.5, 1.0, 1.0)
    orm = np.zeros((n_px, 4), np.float32)
    orm[:] = (1.0, 0.75, 0.0, 1.0)
    bake_stats = {}
    if not skip_bake:
        high = face.high(f"{piece}_high")
        granite = wet_material(f"{piece}_high")
        high.data.materials.append(granite)
        # The granite's noise, lichen floor and soil line read object space, which sits at z < 0 on a
        # fall face: lift the high poly's data 100 m and sink the object as far, so the world shape
        # is unchanged and the object z clears them.
        high.data.transform(Matrix.Translation((0.0, 0.0, 100.0)))
        high.location.z = -100.0
        bpy.context.view_layer.update()
        scn = bpy.context.scene
        scn.world = scn.world or bpy.data.worlds.new(f"{piece}_world")
        bake.setup_cycles(samples=SAMPLES)
        imgs = {k: bake.new_image(f"{piece}.{k}", ATLAS, non_color=k != "albedo") for k in ("albedo", "normal", "ao", "rough")}
        lod0 = lods[0]
        bake.bake_emit_input([high], lod0, "Base Color", imgs["albedo"], cage_extrusion=CAGE, max_ray=MAX_RAY,
                             margin=MARGIN)
        bake.bake_to(lod0, "NORMAL", imgs["normal"], highs=[high], cage_extrusion=CAGE, max_ray=MAX_RAY, margin=MARGIN)
        bake.bake_to(lod0, "ROUGHNESS", imgs["rough"], highs=[high], cage_extrusion=CAGE, max_ray=MAX_RAY,
                     margin=MARGIN)
        occ = face.occluders(granite)
        scn.world.light_settings.distance = AO_DIST
        bake.bake_to(lod0, "AO", imgs["ao"], highs=[high], cage_extrusion=CAGE, max_ray=MAX_RAY, margin=MARGIN,
                     occluders=occ)
        for o in occ:
            bpy.data.objects.remove(o, do_unlink=True)
        bpy.context.view_layer.update()
        px = {}
        for k, im in imgs.items():
            p = bake.save_png(im, str(out / f"{piece}.{k}.tmp.png"))
            px[k] = bake.load_png(p)[1]
            os.remove(p)
        albedo[:] = px["albedo"]
        albedo[:, :3] *= _boulder.ALBEDO_GAIN
        normal[:] = px["normal"]
        orm[:, 0] = px["ao"][:, 0]
        orm[:, 1] = px["rough"][:, 0]
        for arr in (albedo, normal, orm):
            arr[:, 3] = 1.0
        flat = _boulder._islands(lod0, ATLAS)

        def st(v):
            return {"texels": int(v.size), "min": float(v.min()), "mean": float(v.mean()),
                    "p01": float(np.percentile(v, 1)), "p05": float(np.percentile(v, 5))}

        bake_stats["ao"] = st(orm[flat, 0])
        bake_stats["roughness"] = st(orm[flat, 1])
        bake_stats["albedoLuma"] = st(albedo[flat, :3].mean(1))
        bake_stats["normalMissFraction"] = float((normal[flat, 2] < 0.05).mean())
        print(f"{piece}: bake {bake_stats}")
        assert bake_stats["normalMissFraction"] < 0.01, "the normal bake missed the high poly (its faces face the column?)"
        bpy.data.objects.remove(high, do_unlink=True)
        bpy.context.view_layer.update()
    bake.write_png(ATLAS, albedo, str(out / f"{piece}.albedo.png"), non_color=False)
    bake.write_png(NORMAL_SIZE, _rock._downsample(normal, ATLAS, NORMAL_SIZE, renormal=True), str(out / f"{piece}.normal.png"))
    bake.write_png(ORM_SIZE, _rock._downsample(orm, ATLAS, ORM_SIZE), str(out / f"{piece}.orm.png"))
    # Masks from the UV parameters: R snow (less behind the sheet), G wetness near 1, B 0, A 1.
    mu, mv = np.meshgrid((np.arange(MASK_SIZE) + 0.5) / MASK_SIZE, (np.arange(MASK_SIZE) + 0.5) / MASK_SIZE)
    snow = np.full(MASK_SIZE * MASK_SIZE, 0.55)
    wetc = np.full(MASK_SIZE * MASK_SIZE, 0.9)
    for j in range(face.n_strips):
        s = (mu.ravel() - UV_MARGIN) / face.uv_k + face.strip_s0[j]
        v = (mv.ravel() - UV_MARGIN) / face.uv_k - j * (face.H + STRIP_GAP)
        inside = (v >= -0.2) & (v <= face.H + 0.2)
        s_in = np.clip(s[inside], -HALF_W, HALF_W)
        z_in = face.z_of(face.A(top_z(s_in)) + np.clip(v[inside], 0.0, face.H))
        behind = 1.0 - smoothstep(3.6, 4.8, np.abs(s_in))
        snow[inside] = 0.15 + 0.5 * (1.0 - behind)
        wetc[inside] = 0.82 + 0.18 * np.maximum(face.wet(s_in, z_in), face.moss(s_in, z_in))
    bake.make_masks(MASK_SIZE, snow.astype(np.float32), wetc.astype(np.float32), 0.0, None, str(out / f"{piece}.masks.png"))

    col = tier_boxes(face, lods[0], f"{piece}_collider")
    root = scene.empty(piece)
    for o in lods:
        o.parent = root
    col.parent = root
    lips = []
    for t in range(drop):
        p = (A + face.lip_x[t], 0.0, face.lip_z[t])
        export.add_socket(f"socket-lip-{t}", p, root, kind="lip", tier=t, width=SHEET_W,
                          normal=[1.0, 0.0, 0.0], fall=STEP)
        lips.append({"tier": t, "at": [round(c, 4) for c in p], "fall": STEP})
    export.export_glb(root, str(out / f"{piece}.glb"))
    export.write_export_report(str(out / "export-report.json"), root, piece, generator)
    bake.write_bake_report(str(out / "bake-report.json"), receiverSignature=bake.receiver_signature([lods[0]]),
                           device=os.environ.get("HEXW_BAKE_DEVICE", "CPU"), samples=SAMPLES, marginPx=MARGIN,
                           cageExtrusionM=CAGE, maxRayDistanceM=MAX_RAY, aoDistanceM=AO_DIST,
                           channels={"albedo": {"size": ATLAS, "colorspace": "srgb"},
                                     "normal": {"size": NORMAL_SIZE, "colorspace": "linear"},
                                     "orm": {"size": ORM_SIZE, "colorspace": "linear"},
                                     "masks": {"size": MASK_SIZE, "colorspace": "linear"}},
                           texelDensityPxPerM=texel, texelMinPxPerM=texel_min, bakeStats=bake_stats,
                           drop=drop, faceHalfWidthM=HALF_W, reachM=round(float(reach), 3), lips=lips,
                           lodSpecs=[list(x) for x in specs], lod2Drops={}, skipped=skip_bake)
