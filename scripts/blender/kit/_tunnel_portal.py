"""Tunnel portal family: the builder behind road_tunnel_portal.py (P4 elevation; owner-approved reference
assets-source/refs/road_tunnel_portal ref-2, 2026-10-29; method per owner: "create it at the hi res and
bake the textures then reduce poly count").

The piece (ref-2): a head wall of weathered grey granite blocks with a few buff stones, laid in courses in
pale lime mortar, with lichen, moss in the joints and rain streaks; an elliptical arch ringed by dressed
granite voussoirs with a projecting keystone, on dressed jamb stones; a two-course dressed coping with a
drip edge, the hill's turf behind it and ferns over its front; a stone-lined recess fading to black; two
amber wall lanterns on scrolled iron brackets. The banks of the road cutting meet the wall ends (ref-2 has
no wing walls).

Frame: Blender (x, y, z) exports as runtime (x, z, -y); the wall's lateral coordinate s is Blender y and
runtime z = -s. The pivot is the hex centre at the tile's level: the portal is a ground hero at the
origin of the `road_tunnel_portal` tile (wiring W1), never draped. The road enters at edge 3 (-x) through
a cutting, the face looks toward it (-x) and the tunnel runs +x under the raised ground beyond edge 0.

Ground (Q1, the cut row): the game draws `cut_height_at` (hex-core slope's Cutting), the corridor
|z| <= 4 held at the road's level 0, smoothstep over 4 < |z| < 8 to `ground_at`: the C1 edge blend of
corners [1,1,0,0,0,1] x LEVEL_STEP (`height_at`) plus the meadow relief, so the banks carry the Meadow's
hollows and the corridor none (hex-core slope's Plain ground; `check_field` pins both to the Rust samples).
The wall closes edge 0's 16 m (|z| <= 8) up to TOP = 4.0 m, flush with the cover meadow, and its interior
back plane stands 0.2 m inside edge 0 so the cover meadow's skirt never shows.

Texture method. The visible stone is about 60 m², and a unique bake at the gate's 256 px/m covers 16 m²
in the `piece` class's 1024² albedo, so the stone samples shared islands (the oak canopy's and the hay
bales' route): a wrap-tileable wall panel, two voussoirs, a keystone, a jamb and a coping segment, each
Cycles-baked (albedo through EMIT, tangent normal, AO into ORM.R, roughness into ORM.G) from a straight
high-poly template onto a receiver built by the same face functions as the real LODs, so every LOD face
maps onto its island at true scale with the receiver's orientation. The panel template lays its blocks
three periods wide and tall with every block a linked duplicate (one mesh, one local origin, so
`_rock.granite_material`'s object-space noise repeats with it) on a mortar plane whose noise runs on
torus coordinates, and its receiver covers the gutters, so the island wraps (`seam_errors`). Fern fronds
are `_ferns.frond_tile` drawn into their own strip. The colour slots (lining, back, iron, glass) and the
turf take masks from parked or analytic regions of the same masks map.

Pack `core`, kind `prop`, KTX2 class `piece`. LODs are rebuilt from the spec at lower detail, never
decimated: LOD1 drops the voussoir joint sides, the face strips coarsen, the coping halves, the lanterns
lose the scroll brace; LOD2 maps the face and back at 2.5x scale (only LOD0's density is gated), keeps the
voussoir fronts, one coping segment, the lining, the back, a glass box per lantern and four ferns."""
from __future__ import annotations

import json
import math
import os
import pathlib
import random
import sys

import bmesh
import bpy
import numpy as np
from mathutils import Vector

import _ferns
import _props
import _rock
from lib import bake, budgets, collider, export, hexgeo, materials, scene

R = hexgeo.HEX_R
STEP = hexgeo.LEVEL_STEP
APOTHEM = hexgeo.APOTHEM
CORNERS = (1, 1, 0, 0, 0, 1)  # the tile's corner heights (x LEVEL_STEP), corners 0..5
CORRIDOR, BLEND = 4.0, 8.0  # the road corridor half width and the blend's outer edge (|z|, m)


# The tile's corners (runtime x, z): corner k at 30 + 60 k degrees, radius HEX_R.
CS = [(R * math.cos(math.radians(30 + 60 * k)), R * math.sin(math.radians(30 + 60 * k))) for k in range(6)]


def s01(t):
    return t * t * (3.0 - 2.0 * t)


def _edge_ramp(hs, e, x, z):
    """Edge e's eased ramp from its start corner (e + 5) % 6 to its end corner e, at the projection of
    (x, z) onto the edge line, clamped."""
    a, b = CS[(e + 5) % 6], CS[e]
    ex, ez = b[0] - a[0], b[1] - a[1]
    t = max(0.0, min(1.0, ((x - a[0]) * ex + (z - a[1]) * ez) / (ex * ex + ez * ez)))
    h0, h1 = hs[(e + 5) % 6], hs[e]
    return h0 + (h1 - h0) * s01(t)


def height_at(x, z, corners=CORNERS):
    """hex-core slope's Plain field (runtime x, z), metres above the tile's level: the C1 edge blend
    sum_e w_e g_e(t_e), where g_e is edge e's smoothstep-eased ramp between its corners at the clamped
    projection t_e and w_e is proportional to 1 / d_e² to edge e's line, normalised. On or outside
    the hexagon: the nearest edge segment's ramp at the closest point. Ported verbatim from the
    controller's reference (`plain` in .superpowers/sdd/2026-10-28-elevation-first-round/
    task-4-edgeblend-reference.py); `check_field` pins it to the Rust samples."""
    hs = [c * STEP for c in corners]
    d = [APOTHEM - (x * math.cos(math.radians(60 * e)) + z * math.sin(math.radians(60 * e))) for e in range(6)]
    if min(d) <= 0.0:
        best = None
        for e in range(6):
            a, b = CS[(e + 5) % 6], CS[e]
            ex, ez = b[0] - a[0], b[1] - a[1]
            t = max(0.0, min(1.0, ((x - a[0]) * ex + (z - a[1]) * ez) / (ex * ex + ez * ez)))
            q = (a[0] + t * ex, a[1] + t * ez)
            dist = math.hypot(q[0] - x, q[1] - z)
            if best is None or dist < best[0]:
                best = (dist, _edge_ramp(hs, e, q[0], q[1]))
        return best[1]
    w = [1.0 / (de * de) for de in d]
    return sum(wi * _edge_ramp(hs, e, x, z) for e, wi in enumerate(w)) / sum(w)


def meadow_relief(x, z):
    """hex-core slope's `meadow_relief`, the analytic twin of `_terrain.meadow_relief` at Blender
    (x, -z): -0.25 * smoothstep(border distance / 3) * clamp(swell)^2, 0 with zero slope over the last
    3 m of every border (ported verbatim from the controller's reference, Task 7 fix round 1)."""
    bx, by = x, -z
    normals = [(math.cos(math.radians(60 * k)), math.sin(math.radians(60 * k))) for k in range(6)]
    dist = APOTHEM - max(bx * n[0] + by * n[1] for n in normals)
    t = min(max(dist / 3.0, 0.0), 1.0)
    falloff = t * t * (3.0 - 2.0 * t)
    und = 0.5 + 0.5 * (0.6 * math.sin(0.31 * bx + 1.7) * math.cos(0.27 * by - 0.4)
                       + 0.4 * math.sin(0.53 * bx - 0.45 * by + 0.9))
    u = min(max((und - 0.5) / 0.5, 0.0), 1.0)
    return -0.25 * falloff * u * u


def ground_at(x, z):
    """hex-core slope's Plain ground: the edge blend plus the meadow relief."""
    return height_at(x, z) + meadow_relief(x, z)


def cut_height_at(x, z):
    """hex-core slope's Cutting field (Q1, the cut row): 0 for |z| <= 4, a smoothstep blend over
    4 < |z| < 8 to `ground_at` (so the relief rides on the bank side of the blend only), `ground_at`
    beyond (the reference's `cut`)."""
    az = abs(z)
    if az <= CORRIDOR:
        return 0.0
    h = ground_at(x, z)
    if az >= BLEND:
        return h
    return h * s01((az - CORRIDOR) / (BLEND - CORRIDOR))


# hex-core slope's samples at corners [1,1,0,0,0,1] (Rust matches them within 1e-4): (x, z, value). The
# committed slope_samples.json is shared with the Rust test slope::tests::fields_match_python_reference.
_SAMPLES = json.loads((pathlib.Path(__file__).resolve().parent / "slope_samples.json").read_text())
assert _SAMPLES["corners"] == list(CORNERS), "slope_samples.json corners differ from CORNERS"
CUTTING_SAMPLES = [tuple(t) for t in _SAMPLES["cutting"]]
PLAIN_SAMPLES = [tuple(t) for t in _SAMPLES["plain"]]


def check_field():
    """The generator's field against the Rust samples (1e-4), and the wall ends buried under it."""
    errs = [(x, z, "cutting", cut_height_at(x, z), want) for x, z, want in CUTTING_SAMPLES
            if abs(cut_height_at(x, z) - want) > 1e-4]
    errs += [(x, z, "plain", ground_at(x, z), want) for x, z, want in PLAIN_SAMPLES
             if abs(ground_at(x, z) - want) > 1e-4]
    assert not errs, f"field differs from hex-core slope's samples: {errs}"
    ends = min(cut_height_at(X_F - CAP_OUT + (X_E - X_F + CAP_OUT) * i / 40, z) for i in range(41) for z in (-W, W))
    assert ends >= END_B + 0.3, f"the wall end faces start at {END_B} m, the field there falls to {ends:.3f} m"
    return {"cuttingSamples": len(CUTTING_SAMPLES), "plainSamples": len(PLAIN_SAMPLES), "endLineMinM": round(ends, 4)}


# Placement (runtime x): the face, the interior back plane, the wall's back on edge 0.
X_F, X_B, X_E = APOTHEM - 2.2, APOTHEM - 0.2, APOTHEM
# Section (m above the tile's level): the opening's half span, the springing, the arch rise (a
# semi-ellipse), the voussoir ring's radial depth, how far the keystone rises above it, the face top
# (the coping's foot), the wall top, the wall's half width (edge 0 spans |z| <= 8), the bury depth.
A, J, B, RING, KEY_UP = 3.9, 0.6, 2.55, 0.45, 0.12
FACE_TOP, TOP, W, BURY = 3.72, STEP, 8.0, 0.3
CROWN = J + B
assert CROWN <= 3.6, "crown over one level"
assert abs(CROWN + RING + KEY_UP - FACE_TOP) < 1e-9, "the keystone meets the coping"
# Depths (m): voussoir and jamb soffits into the barrel, their projections from the face, the
# keystone's, the coping courses (lower course 0.12 tall projecting 0.06, cap 0.16 projecting 0.13 with
# its top running back to CAP_BACK behind the face), the joint between ring stones.
SOFFIT, V_PROUD, KEY_PROUD = 0.45, 0.04, 0.10
LOW_H, LOW_OUT, CAP_OUT, CAP_BACK = 0.12, 0.06, 0.13, 0.30
# The jamb stones stand on the verge beside the road (the field is 0 there), so they need little bury.
JAMB_BURY = 0.1
assert abs(FACE_TOP + LOW_H + 0.16 - TOP) < 1e-9
RING_JOINT = 0.015
# The face's coursing: eight courses from the road's ground to the coping. Three shared stone strips,
# each PW wide and wrapping along the wall: the foot band (course 0, damp and mossy at its foot), the
# panel (courses 1-3 and 4-6, wrapping vertically too) and the top band (course 7, rain-streaked and
# lichened under the coping's drip). Blocks; mortar plane depth.
CH = FACE_TOP / 8
PW, PH, COURSES = 2.4, 3 * CH, 3
BLOCK_LEN, JOINT, BLOCK_DEEP, MORTAR_D = (0.34, 1.15), 0.024, 0.2, -0.03
BUFF_SHARE = 0.22
# Wall ends: the field there is over 3.4 m, so the end faces start below it at END_B.
END_B = 2.9
# Lanterns: lateral position, glass box (half width, bottom, top), the arm's height and reach.
LAMP_S, LAMP_HW, LAMP_Z0, LAMP_Z1, ARM_Z, ARM_OUT = 5.2, 0.085, 2.06, 2.34, 2.62, 0.42
# Fern clumps (s, on: "coping" over the coping's front or "foot" on the bank at the wall foot), each of
# FRONDS fronds of FERN_L x FERN_W arching out from one root; ref-2 has them at the keystone, along the
# coping and at the wall's foot beside the jambs.
# The coping clumps stand in clusters of two to four with bare coping between (ref-2), each clump at
# its own scale, yaw and lean (FERN_SCALE, FERN_YAW, FERN_LEAN, drawn per clump).
FERN_CLUMPS = [(-7.15, "coping"), (-6.5, "coping"), (-3.0, "coping"), (-2.4, "coping"),
               (0.85, "coping"), (1.4, "coping"), (2.05, "coping"), (5.35, "coping"), (6.0, "coping"),
               (-5.6, "foot"), (-4.7, "foot"), (4.7, "foot"), (5.5, "foot")]
FERN_SCALE, FERN_YAW, FERN_LEAN = (0.6, 1.3), 0.35, (0.7, 1.3)
FRONDS, FERN_VARIANTS = 4, 2
FERN_L, FERN_W = 0.9, 0.30
# The grass fringe over the coping's lip: one card of TUFT x TUFT every FRINGE_STEP along the coping,
# hanging out and down over its front (a continuous soft overhang, ref-2).
FRINGE_S, FRINGE_STEP, FRINGE_LEN = 7.8, (0.32, 0.46), (0.85, 1.0)
TUFT, TUFT_VARIANTS = 0.45, 2
# The barrel lining behind the ring stones: BARREL m of coursed stone (the panel's lower two courses,
# copied into the barrel island and faded with depth), then the dark lining to the back plane.
BARREL, BARREL_COURSES = 0.75, 1
BARREL_ALBEDO_END, BARREL_AO_END = 0.25, 0.35

# Atlas (UV0): ATLAS² albedo; normal, ORM and masks at their own sizes over the same UVs.
ATLAS, NORMAL_SIZE, ORM_SIZE, MASK_SIZE = 1024, 512, 512, 512
PNG_ALBEDO = 512  # the PNG fallback albedo's side (kit-compress pngFallbackSize; owner O1)
TEXEL = 258.0  # px per metre on every shared stone island (the gate's floor is a 256 median)
GUT = 8  # px of gutter round every island (the bake margin fills it, or the wrap does)
SAMPLES, AO_DIST, CAGE, MAX_RAY, MARGIN = 32, 0.12, 0.05, 0.12, GUT

# Per LOD: voussoirs, arch polyline segments per voussoir, face strip width near the ring (m), the
# face and back map scale (cells span scale x the panel), coping segments, lantern detail (2 full,
# 1 box and bracket, 0 box), ferns kept (every k-th), voussoir joint sides, voussoir edge bands.
LOD = [
    {"segs": 15, "sub": 2, "strip": 0.4, "scale": 1.0, "coping": 16, "lamp": 2, "fronds": 4, "frond_segs": 2, "tufts": 1,
     "sides": True, "edges": True},
    {"segs": 9, "sub": 1, "strip": 0.8, "scale": 1.0, "coping": 8, "lamp": 1, "fronds": 2, "frond_segs": 2, "tufts": 2,
     "sides": False, "edges": True},
    {"segs": 7, "sub": 1, "strip": None, "scale": 2.5, "coping": 1, "lamp": 0, "fronds": 1, "frond_segs": 1, "tufts": 4,
     "sides": False, "edges": False},
]
# LOD2 keeps every other fern clump (a clump is over a pixel at 400 m: 1 m fronds).
LOD2_CLUMP_STEP = 3
SLOTS = ("stone", "fern", "turf", "lining", "back", "iron", "glass")


# ---------------------------------------------------------------------------------------------
# Arch geometry
def ellipse_divisions(n):
    """n + 1 parameters theta in [0, pi] at equal arc length along the intrados semi-ellipse."""
    k = 720
    th = [math.pi * i / k for i in range(k + 1)]
    pts = [(A * math.cos(t), B * math.sin(t)) for t in th]
    cum = [0.0]
    for i in range(k):
        cum.append(cum[-1] + math.hypot(pts[i + 1][0] - pts[i][0], pts[i + 1][1] - pts[i][1]))
    out, j = [], 0
    for i in range(n + 1):
        target = cum[-1] * i / n
        while j < k and cum[j + 1] < target:
            j += 1
        f = 0.0 if j >= k else (target - cum[j]) / max(cum[j + 1] - cum[j], 1e-12)
        out.append(th[j] + f * (th[min(j + 1, k)] - th[j]))
    out[0], out[-1] = 0.0, math.pi
    return out


def intrados(t):
    return (A * math.cos(t), J + B * math.sin(t))


def normal(t):
    nx, ny = B * math.cos(t), A * math.sin(t)
    ln = math.hypot(nx, ny)
    return (nx / ln, ny / ln)


def ring_pt(t, r):
    s, h = intrados(t)
    n = normal(t)
    return (s + r * n[0], h + r * n[1])


_EXT = [ring_pt(math.pi * i / 512, RING) for i in range(513)]
KEY_HALF = 0.41  # the keystone's half width at the crown (m, along s)


def keepout(s):
    """The top of the ring (the keystone included) at lateral s, -inf outside it: wall blocks stay
    above it."""
    if abs(s) >= A + RING:
        return -math.inf
    if abs(s) <= KEY_HALF:
        return FACE_TOP
    for (s0, h0), (s1, h1) in zip(_EXT, _EXT[1:]):
        if s1 <= s <= s0:
            f = (s - s0) / (s1 - s0) if s1 != s0 else 0.0
            return h0 + f * (h1 - h0)
    return J


# ---------------------------------------------------------------------------------------------
# Atlas islands
class Island:
    def __init__(self, name, w, h, true_scale=True, px=None):
        self.name, self.w, self.h, self.true_scale = name, w, h, true_scale
        self.pw, self.ph = px if px else (int(math.ceil(w * TEXEL)), int(math.ceil(h * TEXEL)))
        self.x0 = self.y0 = None

    def uv(self, a, b):
        """UV of island coordinates (a, b) in metres from its corner (may run into the gutter)."""
        sx, sy = (TEXEL, TEXEL) if self.true_scale else (self.pw / self.w, self.ph / self.h)
        return ((self.x0 + GUT + a * sx) / ATLAS, (self.y0 + GUT + b * sy) / ATLAS)

    def px_box(self, gutter=True):
        g = 0 if gutter else GUT
        return (self.x0 + g, self.y0 + g, self.x0 + self.pw + 2 * GUT - g, self.y0 + self.ph + 2 * GUT - g)


def _islands():
    isl = [Island("panel", PW, PH), Island("band_top", PW, CH), Island("band_foot", PW, CH)]
    for v in ("v0", "v1"):
        isl += [Island(f"{v}_front", 0.75, RING), Island(f"{v}_soffit", 0.75, V_PROUD + SOFFIT),
                Island(f"{v}_band", 0.75, V_PROUD), Island(f"{v}_side0", RING, V_PROUD),
                Island(f"{v}_side1", RING, V_PROUD)]
    isl += [Island("key_front", 2 * KEY_HALF, RING + KEY_UP), Island("key_band", 2 * KEY_HALF, KEY_PROUD),
            Island("key_side0", RING + KEY_UP, KEY_PROUD), Island("key_side1", RING + KEY_UP, KEY_PROUD)]
    isl += [Island("jamb_front", RING, J + JAMB_BURY), Island("jamb_inner", V_PROUD + SOFFIT, J + JAMB_BURY),
            Island("jamb_band", V_PROUD, J + JAMB_BURY), Island("jamb_top", RING, V_PROUD)]
    lc = 2 * W / LOD[0]["coping"]
    isl += [Island("cop_lfront", lc, LOW_H), Island("cop_lunder", lc, LOW_OUT),
            Island("cop_cunder", lc, CAP_OUT - LOW_OUT), Island("cop_cfront", lc, TOP - FACE_TOP - LOW_H),
            Island("cop_ctop", lc, CAP_OUT + CAP_BACK), Island("cop_end", CAP_OUT, TOP - FACE_TOP)]
    isl += [Island(f"fern{k}", FERN_L, FERN_W) for k in range(FERN_VARIANTS)]
    isl += [Island(f"tuft{k}", TUFT, TUFT) for k in range(TUFT_VARIANTS)]
    isl += [Island("barrel", BARREL, BARREL_COURSES * CH)]
    isl += [Island("lining", 1.0, 1.0, False, (32, 32)), Island("back", 1.0, 1.0, False, (32, 32))]
    isl += [Island(f"park_{n}", 1.0, 1.0, False, (16, 16)) for n in ("turf", "iron", "glass")]
    return isl


def pack(islands):
    """Skyline bottom-left packing, largest area first (ties by name, so the layout is fixed); every
    island keeps GUT px of gutter on each side."""
    sky = [(0, 0, ATLAS)]  # (x, y, width) segments of the skyline, left to right
    order = sorted(islands, key=lambda i: (-(i.pw + 2 * GUT) * (i.ph + 2 * GUT), i.name))
    for i in order:
        w, h = i.pw + 2 * GUT, i.ph + 2 * GUT
        best = None
        for k, (x, _, _) in enumerate(sky):
            if x + w > ATLAS:
                break
            top, span, j = 0, 0, k
            while span < w:
                top = max(top, sky[j][1])
                span += sky[j][2]
                j += 1
            if top + h <= ATLAS and (best is None or (top, x) < (best[0], best[1])):
                best = (top, x)
        assert best is not None, f"atlas overflow at {i.name} ({w}x{h} px)"
        y, x = best
        i.x0, i.y0 = x, y
        new, done = [], False
        for sx, sy, sw in sky:
            a, b = max(sx, x), min(sx + sw, x + w)
            if a >= b:
                new.append((sx, sy, sw))
                continue
            if sx < a:
                new.append((sx, sy, a - sx))
            if not done:
                new.append((x, y + h, w))
                done = True
            if b < sx + sw:
                new.append((b, sy, sx + sw - b))
        merged = []
        for seg in sorted(new):
            if merged and merged[-1][1] == seg[1] and merged[-1][0] + merged[-1][2] == seg[0]:
                merged[-1] = (merged[-1][0], seg[1], merged[-1][2] + seg[2])
            else:
                merged.append(seg)
        sky = merged
    return {i.name: i for i in islands}


# ---------------------------------------------------------------------------------------------
# Mesh building with explicit UVs
class Builder:
    def __init__(self, slots):
        self.bm = bmesh.new()
        self.uv = self.bm.loops.layers.uv.new("UVMap")
        self.idx = {n: i for i, n in enumerate(slots)}

    def face(self, pts, uvs, slot, expect=None):
        f = self.bm.faces.new([self.bm.verts.new(p) for p in pts])
        f.material_index = self.idx[slot]
        for loop, uv in zip(f.loops, uvs):
            loop[self.uv].uv = uv
        if expect is not None:
            f.normal_update()
            if f.normal.dot(Vector(expect)) < 0:
                f.normal_flip()
        return f

    def obj(self, name, mats):
        bmesh.ops.triangulate(self.bm, faces=self.bm.faces)
        mesh = bpy.data.meshes.new(name)
        self.bm.to_mesh(mesh)
        self.bm.free()
        o = scene.link(bpy.data.objects.new(name, mesh))
        for m in mats:
            o.data.materials.append(m)
        for p in o.data.polygons:
            p.use_smooth = False
        return o


def blender(x, s, h):
    """Blender point of runtime x, lateral s (Blender y) and height h."""
    return (x, s, h)


# ---------------------------------------------------------------------------------------------
# Face functions shared by the receivers and the LODs. Each takes a point map P and an island map and
# emits faces; the receivers pass straight maps, the LODs the arch, the face plane or the coping line.
def panel_cell(b, P, isl, u0, u1, v0, v1, slot="stone", bottom=None, scale=1.0, expect=None, name="panel"):
    """One cell of a panel-mapped surface: P(u, v) -> point, (u, v) in metres on the panel (u to the
    viewer's right, v up), mapped at 1 / scale. `bottom(u)` cuts the cell's bottom edge."""
    if bottom is None:
        pts = [(u0, v0), (u1, v0), (u1, v1), (u0, v1)]
    else:
        b0, b1 = max(v0, bottom(u0)), max(v0, bottom(u1))
        lim = v1 - 0.04
        if b0 >= lim and b1 >= lim:
            return
        if b0 >= lim:
            um = u0 + (b0 - v1) / (b0 - b1) * (u1 - u0)
            pts = [(um, v1), (u1, b1), (u1, v1)]
        elif b1 >= lim:
            um = u0 + (v1 - b0) / (b1 - b0) * (u1 - u0)
            pts = [(u0, b0), (um, v1), (u0, v1)]
        else:
            pts = [(u0, b0), (u1, b1), (u1, v1), (u0, v1)]
    b.face([P(u, v) for u, v in pts], [isl[name].uv(u / scale, v / scale) for u, v in pts], slot, expect)


def ring_stone(b, P, isl, pre, length, depth, proud, sub, sides, edges, soffit_island=None, soffit=True):
    """A ring stone in its own parameters: P(f, r, dd) -> point, f in [0, 1] along it, r radial (0 at
    the intrados, `depth` at its top), dd toward the viewer (`proud` at its front, -SOFFIT inside).
    Emits the front, the top band, the soffit (unless `soffit` is off: the keystone's template, whose
    soffit island is v0's) and (sides) the joint sides on islands `pre`_*."""
    ff = [q / sub for q in range(sub + 1)]
    soff = isl[soffit_island or f"{pre}_soffit"] if soffit else None
    for fa, fb in zip(ff, ff[1:]):
        quad = [(fa, 0.0), (fb, 0.0), (fb, depth), (fa, depth)]
        b.face([P(f, r, proud) for f, r in quad], [isl[f"{pre}_front"].uv(f * length, r) for f, r in quad],
               "stone", P.front)
        if edges:
            band = [(fa, 0.0), (fb, 0.0), (fb, proud), (fa, proud)]
            b.face([P(f, depth, dd) for f, dd in band], [isl[f"{pre}_band"].uv(f * length, dd) for f, dd in band],
                   "stone", P.out((fa + fb) / 2))
        if soffit:
            sof = [(fa, -SOFFIT), (fb, -SOFFIT), (fb, proud), (fa, proud)]
            b.face([P(f, 0.0, dd) for f, dd in sof], [soff.uv(f * soff.w, proud - dd) for f, dd in sof], "stone",
                   P.inward((fa + fb) / 2))
    if sides:
        for f, k in ((0.0, 0), (1.0, 1)):
            q = [(0.0, 0.0), (depth, 0.0), (depth, proud), (0.0, proud)]
            b.face([P(f, r, dd) for r, dd in q], [isl[f"{pre}_side{k}"].uv(r, dd) for r, dd in q], "stone",
                   P.side(f))


def jamb_stone(b, P, isl, mirror):
    """A jamb stone: P(u, v, dd) with u across its front (0 at the outer edge), v up from -BURY."""
    top = J + JAMB_BURY - RING_JOINT
    m = (lambda u: RING - u) if mirror else (lambda u: u)
    q = [(0.0, 0.0), (RING, 0.0), (RING, top), (0.0, top)]
    b.face([P(u, v, V_PROUD) for u, v in q], [isl["jamb_front"].uv(m(u), v) for u, v in q], "stone", P.front)
    inner = [(-SOFFIT, 0.0), (V_PROUD, 0.0), (V_PROUD, top), (-SOFFIT, top)]
    b.face([P(RING, v, dd) for dd, v in inner], [isl["jamb_inner"].uv(V_PROUD - dd, v) for dd, v in inner], "stone",
           P.inner)
    band = [(0.0, 0.0), (V_PROUD, 0.0), (V_PROUD, top), (0.0, top)]
    b.face([P(0.0, v, dd) for dd, v in band], [isl["jamb_band"].uv(dd, v) for dd, v in band], "stone", P.outer)
    tq = [(0.0, 0.0), (RING, 0.0), (RING, V_PROUD), (0.0, V_PROUD)]
    b.face([P(u, top, dd) for u, dd in tq], [isl["jamb_top"].uv(m(u), dd) for u, dd in tq], "stone", P.up)


def coping_segment(b, P, isl, length, ends=(False, False)):
    """A coping segment: P(u, h, dd) with u along it (0 at the viewer's left), h above FACE_TOP, dd
    toward the viewer from the face plane. Faces on the cop_* islands, u scaled onto their length."""
    k = isl["cop_lfront"].w / length
    up, down, fwd = P.up, P.down, P.front
    ch = TOP - FACE_TOP
    rows = [
        ("cop_lfront", [(0.0, 0.0, LOW_OUT), (length, 0.0, LOW_OUT), (length, LOW_H, LOW_OUT), (0.0, LOW_H, LOW_OUT)],
         lambda u, h, dd: (u * k, h), fwd),
        ("cop_lunder", [(0.0, 0.0, 0.0), (length, 0.0, 0.0), (length, 0.0, LOW_OUT), (0.0, 0.0, LOW_OUT)],
         lambda u, h, dd: (u * k, dd), down),
        ("cop_cunder", [(0.0, LOW_H, LOW_OUT), (length, LOW_H, LOW_OUT), (length, LOW_H, CAP_OUT), (0.0, LOW_H, CAP_OUT)],
         lambda u, h, dd: (u * k, dd - LOW_OUT), down),
        ("cop_cfront", [(0.0, LOW_H, CAP_OUT), (length, LOW_H, CAP_OUT), (length, ch, CAP_OUT), (0.0, ch, CAP_OUT)],
         lambda u, h, dd: (u * k, h - LOW_H), fwd),
        ("cop_ctop", [(0.0, ch, CAP_OUT), (length, ch, CAP_OUT), (length, ch, -CAP_BACK), (0.0, ch, -CAP_BACK)],
         lambda u, h, dd: (u * k, CAP_OUT - dd), up),
    ]
    for name, quad, m, expect in rows:
        b.face([P(*q) for q in quad], [isl[name].uv(*m(*q)) for q in quad], "stone", expect)
    for u, on, side in ((0.0, ends[0], P.left), (length, ends[1], P.right)):
        if not on:
            continue
        prof = [(0.0, 0.0), (LOW_OUT, 0.0), (LOW_OUT, LOW_H), (CAP_OUT, LOW_H), (CAP_OUT, ch), (0.0, ch)]
        for tri in ((0, 1, 2), (0, 2, 5), (2, 3, 4), (2, 4, 5)):
            pts = [prof[i] for i in tri]
            b.face([P(u, h, dd) for dd, h in pts], [isl["cop_end"].uv(dd, h) for dd, h in pts], "stone", side)


# ---------------------------------------------------------------------------------------------
# Point maps. Each is a callable with face-direction hints (Blender vectors) as attributes.
class FaceMap:
    """The head wall's face plane x = X_F: (u, v) on a panel cell whose right edge (u = 0) is s_c."""

    def __init__(self, s_c, h_c, x=X_F):
        self.s_c, self.h_c, self.x = s_c, h_c, x

    def __call__(self, u, v):
        return blender(self.x, self.s_c - u, self.h_c + v)


class ArchMap:
    """A ring stone from theta t0 to t1 on the arch (f along it, r radial, dd toward the viewer)."""

    def __init__(self, t0, t1):
        self.t0, self.t1 = t0, t1
        self.front = (-1.0, 0.0, 0.0)

    def t(self, f):
        return self.t0 + f * (self.t1 - self.t0)

    def __call__(self, f, r, dd):
        s, h = ring_pt(self.t(f), r)
        return blender(X_F - dd, s, h)

    def out(self, f):
        n = normal(self.t(f))
        return (0.0, n[0], n[1])

    def inward(self, f):
        n = normal(self.t(f))
        return (0.0, -n[0], -n[1])

    def side(self, f):
        t = self.t(f)
        tg = (-A * math.sin(t), B * math.cos(t))
        sg = -1.0 if f == 0.0 else 1.0
        return (0.0, tg[0] * sg, tg[1] * sg)


class StraightRing:
    """The template's straight ring stone at origin o: f along -y (the viewer's right), r up."""

    def __init__(self, o, length):
        self.o, self.length = o, length
        self.front = (-1.0, 0.0, 0.0)

    def __call__(self, f, r, dd):
        return (self.o[0] - dd, self.o[1] - f * self.length, self.o[2] + r)

    def out(self, f):
        return (0.0, 0.0, 1.0)

    def inward(self, f):
        return (0.0, 0.0, -1.0)

    def side(self, f):
        return (0.0, 1.0, 0.0) if f == 0.0 else (0.0, -1.0, 0.0)


class JambMap:
    """A jamb stone: u across its front from its outer edge, v up from -BURY, dd toward the viewer.
    side +1 is the jamb at s > 0 (outer edge s = A + RING), -1 the one at s < 0."""

    def __init__(self, side, o=None):
        self.side, self.o = side, o
        self.front = (-1.0, 0.0, 0.0)
        self.up = (0.0, 0.0, 1.0)
        if o is None:
            self.inner, self.outer = (0.0, -side, 0.0), (0.0, side, 0.0)
        else:
            self.inner, self.outer = (0.0, -1.0, 0.0), (0.0, 1.0, 0.0)

    def __call__(self, u, v, dd):
        if self.o is not None:
            return (self.o[0] - dd, self.o[1] - u, self.o[2] + v)
        s = self.side * (A + RING - u)
        return blender(X_F - dd, s, v - JAMB_BURY)


class CopingMap:
    """A coping segment whose left end (u = 0, the viewer's left) is at lateral s0, or a template."""

    def __init__(self, s0, o=None):
        self.s0, self.o = s0, o
        self.front, self.up, self.down = (-1.0, 0.0, 0.0), (0.0, 0.0, 1.0), (0.0, 0.0, -1.0)
        self.left, self.right = (0.0, 1.0, 0.0), (0.0, -1.0, 0.0)

    def __call__(self, u, h, dd):
        if self.o is not None:
            return (self.o[0] - dd, self.o[1] - u, self.o[2] + h)
        return blender(X_F - dd, self.s0 - u, FACE_TOP + h)


# ---------------------------------------------------------------------------------------------
# High-poly templates and their receivers
TEMPLATE_X = 300.0  # templates stand far from the piece and from each other


def _bevel_block(bm, lo, hi, offset, seg=2):
    """An axis-aligned box from lo to hi (Blender coords) with bevelled edges, added to bm."""
    c = [(lo[0] + hi[0]) / 2, (lo[1] + hi[1]) / 2, (lo[2] + hi[2]) / 2]
    size = [hi[i] - lo[i] for i in range(3)]
    res = bmesh.ops.create_cube(bm, size=1.0)
    for v in res["verts"]:
        v.co = Vector((c[0] + v.co.x * size[0], c[1] + v.co.y * size[1], c[2] + v.co.z * size[2]))
    edges = sorted({e for v in res["verts"] for e in v.link_edges},
                   key=lambda e: tuple(sorted((round(v.co.x, 5), round(v.co.y, 5), round(v.co.z, 5)) for v in e.verts)))
    bmesh.ops.bevel(bm, geom=edges, offset=offset, segments=seg, affect="EDGES", profile=0.6)


def rough_bump(mat, scale=7.0, strength=0.35, distance=0.012):
    """A rough-faced mid-scale bump under the material's grain bump."""
    nt = mat.node_tree
    bsdf = next(n for n in nt.nodes if n.type == "BSDF_PRINCIPLED")
    old = next(l for l in nt.links if l.to_socket == bsdf.inputs["Normal"])
    grain = old.from_node
    tc = nt.nodes.new("ShaderNodeTexCoord")
    nz = nt.nodes.new("ShaderNodeTexNoise")
    nz.inputs["Scale"].default_value, nz.inputs["Detail"].default_value = scale, 3.0
    nt.links.new(tc.outputs["Object"], nz.inputs["Vector"])
    bump = nt.nodes.new("ShaderNodeBump")
    bump.inputs["Strength"].default_value, bump.inputs["Distance"].default_value = strength, distance
    nt.links.new(nz.outputs["Fac"], bump.inputs["Height"])
    nt.links.new(bump.outputs["Normal"], grain.inputs["Normal"])


class _Nodes:
    """Node-graph helpers over one material instance: layers mix into its base colour."""

    def __init__(self, mat):
        self.nt = mat.node_tree
        self.N, self.L = self.nt.nodes, self.nt.links
        self.bsdf = next(n for n in self.N if n.type == "BSDF_PRINCIPLED")
        self.tc = self.N.new("ShaderNodeTexCoord")
        self.geo = self.N.new("ShaderNodeNewGeometry")

    def coord(self, kind, stretch=None):
        """'Object' or 'Generated' texture space, or 'Position' (world); `stretch` scales it per axis."""
        s = self.geo.outputs["Position"] if kind == "Position" else self.tc.outputs[kind]
        if stretch:
            v = self.N.new("ShaderNodeVectorMath")
            v.operation = "MULTIPLY"
            v.inputs[1].default_value = stretch
            self.L.new(s, v.inputs[0])
            s = v.outputs[0]
        return s

    def noise(self, scale, detail, kind="Object", stretch=None):
        t = self.N.new("ShaderNodeTexNoise")
        t.inputs["Scale"].default_value, t.inputs["Detail"].default_value = scale, detail
        self.L.new(self.coord(kind, stretch), t.inputs["Vector"])
        return t.outputs["Fac"]

    def axis(self, kind, i):
        sep = self.N.new("ShaderNodeSeparateXYZ")
        self.L.new(self.coord(kind), sep.inputs[0])
        return sep.outputs[i]

    def rng(self, fac, a, b):
        r = self.N.new("ShaderNodeMapRange")
        r.inputs["From Min"].default_value, r.inputs["From Max"].default_value = a, b
        r.clamp = True
        self.L.new(fac, r.inputs["Value"])
        return r.outputs["Result"]

    def math(self, op, a, b):
        n = self.N.new("ShaderNodeMath")
        n.operation = op
        n.use_clamp = op in ("ADD", "MULTIPLY_ADD")
        for i, x in enumerate((a, b)):
            if isinstance(x, (int, float)):
                n.inputs[i].default_value = float(x)
            else:
                self.L.new(x, n.inputs[i])
        return n.outputs[0]

    def mix(self, fac, a, b):
        n = self.N.new("ShaderNodeMix")
        n.data_type = "RGBA"
        ins = {s.identifier: s for s in n.inputs}
        if isinstance(fac, (int, float)):
            ins["Factor_Float"].default_value = float(fac)
        else:
            self.L.new(fac, ins["Factor_Float"])
        for key, x in (("A_Color", a), ("B_Color", b)):
            if isinstance(x, tuple):
                ins[key].default_value = x
            else:
                self.L.new(x, ins[key])
        return next(s for s in n.outputs if s.identifier == "Result_Color")

    def over_base(self, fac, colour):
        """Mix the base colour toward `colour` (a linear RGBA tuple or a socket) by `fac`."""
        link = next(l for l in self.L if l.to_socket == self.bsdf.inputs["Base Color"])
        src = link.from_socket
        self.L.remove(link)
        self.L.new(self.mix(fac, src, colour), self.bsdf.inputs["Base Color"])


def lichen_layer(mat, boost=None):
    """ref-2's lichen as clustered blooms with soft edges over this material instance's base colour:
    object-space blooms (a mid noise through a wide soft ramp) inside larger clusters, pale grey-green
    and mustard. `boost(nodes)` returns a 0..1 socket that thickens it where water runs. The approved
    _rock helper stays untouched."""
    n = _Nodes(mat)
    bloom = n.math("MULTIPLY", n.rng(n.noise(3.2, 3.0), 0.47, 0.61), n.rng(n.noise(0.9, 2.0), 0.40, 0.62))
    if boost is not None:
        bloom = n.math("ADD", bloom, n.math("MULTIPLY", boost(n), n.rng(n.noise(4.5, 3.0), 0.36, 0.62)))
    col = n.mix(n.rng(n.noise(2.2, 1.0), 0.42, 0.58), materials.srgb("#a9ae9a"), materials.srgb("#bdb455"))
    n.over_base(bloom, col)


def water_top(z_top, reach):
    """A boost that is 1 at world height z_top and fades to 0 `reach` below it (runoff under the coping)."""
    return lambda n: n.rng(n.axis("Position", 2), z_top - reach, z_top)


def water_foot(z_foot, reach):
    """A boost that is 1 at world height z_foot and fades to 0 `reach` above it (damp at the wall foot)."""
    return lambda n: n.rng(n.axis("Position", 2), z_foot + reach, z_foot)


def runoff_layer(mat, wet):
    """Where `wet(nodes)` is high: dark rain streaks (object-space noise stretched down the wall) and
    moss in clumps. Rain streaks run from the coping, so the top band takes this with water_top."""
    n = _Nodes(mat)
    w = wet(n)
    streak = n.rng(n.noise(5.0, 4.0, stretch=(1.0, 1.0, 0.12)), 0.46, 0.72)
    n.over_base(n.math("MULTIPLY", n.math("MULTIPLY", w, streak), 0.55), materials.srgb("#3c3a35"))
    moss = n.rng(n.noise(4.0, 3.0), 0.48, 0.62)
    n.over_base(n.math("MULTIPLY", n.math("MULTIPLY", w, moss), 0.8), materials.srgb("#55702a"))


def damp_layer(mat, wet):
    """Where `wet(nodes)` is high: the damp darkening and green moss of a wall's foot."""
    n = _Nodes(mat)
    w = wet(n)
    n.over_base(n.math("MULTIPLY", w, 0.35), materials.srgb("#46443d"))
    moss = n.rng(n.noise(3.5, 3.0), 0.42, 0.60)
    n.over_base(n.math("MULTIPLY", n.math("MULTIPLY", w, moss), 0.9), materials.srgb("#4b6424"))


def joint_wet(n):
    """A ring stone's joints and top (Generated coordinates of its straight template block: y along
    the arch, z radial), where water runs between voussoirs and sits on the extrados."""
    gy = n.axis("Generated", 1)
    edge = n.math("MINIMUM", gy, n.math("SUBTRACT", 1.0, gy))
    joints = n.rng(edge, 0.14, 0.0)
    top = n.rng(n.axis("Generated", 2), 0.8, 1.0)
    return n.math("ADD", joints, n.math("MULTIPLY", top, 0.6))


def tinted(mat, name, rgb):
    """A copy of `mat` whose base colour is multiplied by rgb (the buff stones)."""
    m = mat.copy()
    m.name = name
    nt = m.node_tree
    bsdf = next(n for n in nt.nodes if n.type == "BSDF_PRINCIPLED")
    link = next(l for l in nt.links if l.to_socket == bsdf.inputs["Base Color"])
    src = link.from_socket
    nt.links.remove(link)
    mix = nt.nodes.new("ShaderNodeMix")
    mix.data_type, mix.blend_type = "RGBA", "MULTIPLY"
    ins = {s.identifier: s for s in mix.inputs}
    ins["Factor_Float"].default_value = 1.0
    nt.links.new(src, ins["A_Color"])
    ins["B_Color"].default_value = (*rgb, 1.0)
    nt.links.new(next(s for s in mix.outputs if s.identifier == "Result_Color"), bsdf.inputs["Base Color"])
    return m


def mortar_material(name):
    """Pale lime mortar with grime and moss, on torus coordinates of a plane whose UV runs one panel
    period per unit, so the panel wraps."""
    m = bpy.data.materials.new(name)
    nt = materials.ensure_nodes(m)
    N, L = nt.nodes, nt.links
    bsdf = next(n for n in N if n.type == "BSDF_PRINCIPLED")
    vec, w = materials.torus_coords(nt, 9.0)

    def noise4(scale_mul, detail):
        t = N.new("ShaderNodeTexNoise")
        t.noise_dimensions = "4D"
        t.inputs["Detail"].default_value = detail
        mul = N.new("ShaderNodeVectorMath")
        mul.operation = "SCALE"
        mul.inputs["Scale"].default_value = scale_mul
        L.new(vec, mul.inputs[0])
        L.new(mul.outputs[0], t.inputs["Vector"])
        mw = N.new("ShaderNodeMath")
        mw.operation = "MULTIPLY"
        mw.inputs[1].default_value = scale_mul
        L.new(w, mw.inputs[0])
        L.new(mw.outputs[0], t.inputs["W"])
        t.inputs["Scale"].default_value = 1.0
        return t.outputs["Fac"]

    def ramp(fac, a, b, c0, c1):
        r = N.new("ShaderNodeMapRange")
        r.inputs["From Min"].default_value, r.inputs["From Max"].default_value = a, b
        r.clamp = True
        L.new(fac, r.inputs["Value"])
        mix = N.new("ShaderNodeMix")
        mix.data_type = "RGBA"
        ins = {s.identifier: s for s in mix.inputs}
        L.new(r.outputs["Result"], ins["Factor_Float"])
        for key, col in (("A_Color", c0), ("B_Color", c1)):
            if isinstance(col, tuple):
                ins[key].default_value = col
            else:
                L.new(col, ins[key])
        return next(s for s in mix.outputs if s.identifier == "Result_Color")

    c = materials.srgb
    base = ramp(noise4(1.0, 3.0), 0.35, 0.65, c("#b9b096"), c("#d3caae"))
    grime = ramp(noise4(0.6, 2.0), 0.45, 0.7, base, c("#8f8670"))
    moss = ramp(noise4(1.6, 4.0), 0.47, 0.58, grime, c("#4d6526"))
    L.new(moss, bsdf.inputs["Base Color"])
    bsdf.inputs["Roughness"].default_value = 0.95
    return m


TOP_BAND_Z, FOOT_BAND_Z = 12.0, 24.0  # the band templates' world heights (their weathering keys on them)


def stone_materials(piece):
    """The template materials: the granite with lichen blooms and its buff tint (the panel); the same
    with rain runoff and thicker lichen under the coping (the top band) and with damp and moss at the
    foot (the foot band); the ring stones' granite weathered at their joints and tops; the lime mortar
    with each strip's weathering."""
    grey = _rock.granite_material(f"{piece}_granite", soil_z=-1000.0)
    rough_bump(grey)
    lichen_layer(grey)
    buff = tinted(grey, f"{piece}_granite_buff", (1.18, 1.0, 0.74))
    mortar = mortar_material(f"{piece}_mortar")

    def variant(mat, tag, layers):
        m = mat.copy()
        m.name = f"{mat.name}_{tag}"
        for layer in layers:
            layer(m)
        return m

    top_wet = water_top(TOP_BAND_Z + CH, 0.45)
    foot_wet = water_foot(FOOT_BAND_Z, 0.40)
    top_stone = [lambda m: lichen_layer(m, boost=top_wet), lambda m: runoff_layer(m, top_wet)]
    foot_stone = [lambda m: lichen_layer(m, boost=foot_wet), lambda m: damp_layer(m, foot_wet)]
    return {
        "grey": grey,
        "panel": (grey, buff, mortar),
        "band_top": (variant(grey, "top", top_stone), variant(buff, "top", top_stone),
                     variant(mortar, "top", [lambda m: runoff_layer(m, top_wet)])),
        "band_foot": (variant(grey, "foot", foot_stone), variant(buff, "foot", foot_stone),
                      variant(mortar, "foot", [lambda m: damp_layer(m, foot_wet)])),
        "ring": variant(grey, "ring", [lambda m: lichen_layer(m, boost=joint_wet),
                                       lambda m: runoff_layer(m, lambda n: n.math("MULTIPLY", joint_wet(n), 0.7))]),
    }


def build_templates(isl, mats):
    """The high-poly templates and one receiver object holding every baked island. `mats` maps each
    wall strip to its (grey, buff, mortar) instances and "ring" / "grey" to the ring stones' and the
    coping's. Returns (highs, receiver)."""
    highs = []
    rb = Builder(SLOTS)
    rng = random.Random("road_tunnel_portal-templates")
    grey = mats["grey"]

    def block_object(name, lo, hi, bevel, mat, offset):
        bm = bmesh.new()
        _bevel_block(bm, [lo[i] - offset[i] for i in range(3)], [hi[i] - offset[i] for i in range(3)], bevel)
        me = bpy.data.meshes.new(name)
        bm.to_mesh(me)
        bm.free()
        me.materials.append(mat)
        o = scene.link(bpy.data.objects.new(name, me))
        o.location = offset
        return o

    def wall_strip(name, o, courses, wrap_v):
        """Courses of blocks for strip island `name` at template origin o, three periods along (and,
        if wrap_v, three up), every copy a linked duplicate, on a torus-noise mortar plane."""
        s_grey, s_buff, s_mortar = mats[name]
        height = courses * CH
        ups = (-1, 0, 1) if wrap_v else (0,)
        for k in range(courses):
            lens = []
            while sum(lens) < PW:
                lens.append(rng.uniform(*BLOCK_LEN))
            f = PW / sum(lens)
            lens = [x * f for x in lens]
            u = rng.uniform(0.0, PW)
            for i, ln in enumerate(lens):
                inset_lo, inset_hi = rng.uniform(0.0, 0.012), rng.uniform(0.0, 0.012)
                proud = rng.uniform(-0.012, 0.012)
                mat = s_buff if rng.random() < BUFF_SHARE else s_grey
                seed_off = (rng.uniform(-50, 50), rng.uniform(-50, 50), rng.uniform(-50, 50))
                bevel = rng.uniform(0.018, 0.032)
                u0, u1 = u + JOINT / 2, u + ln - JOINT / 2
                v0, v1 = k * CH + JOINT / 2 + inset_lo, (k + 1) * CH - JOINT / 2 - inset_hi
                base = None
                for m_ in ups:
                    for n_ in (-2, -1, 0, 1):
                        du, dv = n_ * PW, m_ * height
                        lo = (o[0] - proud, o[1] - (u1 + du), o[2] + v0 + dv)
                        hi = (o[0] - proud + BLOCK_DEEP, o[1] - (u0 + du), o[2] + v1 + dv)
                        if base is None:
                            base = block_object(f"tp_{name}_{k}_{i}", lo, hi, bevel, mat,
                                                ((lo[0] + hi[0]) / 2 + seed_off[0], (lo[1] + hi[1]) / 2 + seed_off[1],
                                                 (lo[2] + hi[2]) / 2 + seed_off[2]))
                            base_lo = lo
                            highs.append(base)
                        else:
                            dup = base.copy()
                            dup.location = (base.location[0] + lo[0] - base_lo[0], base.location[1] + lo[1] - base_lo[1],
                                            base.location[2] + lo[2] - base_lo[2])
                            scene.link(dup)
                            highs.append(dup)
                u += ln
        # The mortar plane behind, UV one period per unit for the torus noise.
        me = bpy.data.meshes.new(f"tp_{name}_mortar")
        bm = bmesh.new()
        uvl = bm.loops.layers.uv.new("UVMap")
        lo_v, hi_v = (-height, 2 * height) if wrap_v else (-0.1, height + 0.1)
        corners = [(-2 * PW, lo_v), (2 * PW, lo_v), (2 * PW, hi_v), (-2 * PW, hi_v)]
        fm = bm.faces.new([bm.verts.new((o[0] - MORTAR_D, o[1] - u, o[2] + v)) for u, v in corners])
        for loop, (u, v) in zip(fm.loops, corners):
            loop[uvl].uv = (u / PW, v / PH)
        fm.normal_update()
        if fm.normal.x > 0:
            fm.normal_flip()
        bm.to_mesh(me)
        bm.free()
        me.materials.append(s_mortar)
        highs.append(scene.link(bpy.data.objects.new(f"tp_{name}_mortar", me)))
        g = GUT / TEXEL
        v_lo, v_hi = (-g, height + g) if wrap_v else (0.0, height)
        panel_cell(rb, FaceMap(o[1], o[2], x=o[0]), isl, -g, PW + g, v_lo, v_hi, name=name)

    wall_strip("panel", (TEMPLATE_X, 0.0, 0.0), COURSES, True)
    # The top band under a coping slab (its drip shades the course), the foot band on the ground.
    o_top = (TEMPLATE_X, 0.0, TOP_BAND_Z)
    wall_strip("band_top", o_top, 1, False)
    highs.append(block_object("tp_band_top_cap", (o_top[0] - CAP_OUT, o_top[1] - 3 * PW, o_top[2] + CH),
                              (o_top[0] + 0.6, o_top[1] + 2 * PW, o_top[2] + CH + 0.28), 0.015, grey,
                              (o_top[0] + 21.0, o_top[1] + 5.0, o_top[2] + 7.0)))
    o_foot = (TEMPLATE_X, 0.0, FOOT_BAND_Z)
    wall_strip("band_foot", o_foot, 1, False)
    highs.append(block_object("tp_band_foot_ground", (o_foot[0] - 1.5, o_foot[1] - 3 * PW, o_foot[2] - 0.3),
                              (o_foot[0] + 0.3, o_foot[1] + 2 * PW, o_foot[2] - 0.02), 0.01, grey,
                              (o_foot[0] + 17.0, o_foot[1] - 4.0, o_foot[2] - 6.0)))

    # Ring stones (two voussoirs, the keystone), alone: a neighbour inside a side receiver's cage would
    # bake its own interior there.
    for idx, (pre, length, depth, proud) in enumerate((("v0", 0.75, RING, V_PROUD), ("v1", 0.75, RING, V_PROUD),
                                                      ("key", 2 * KEY_HALF, RING + KEY_UP, KEY_PROUD))):
        oo = (TEMPLATE_X, 20.0 + 6.0 * idx, 0.0)
        lo = (oo[0] - proud, oo[1] - length, oo[2])
        hi = (oo[0] + SOFFIT, oo[1], oo[2] + depth)
        highs.append(block_object(f"tp_{pre}", lo, hi, 0.028, mats["ring"], (lo[0] + 7.0 * idx, lo[1] + 3.0, lo[2] - 5.0 * idx)))
        ring_stone(rb, StraightRing(oo, length), isl, pre, length, depth, proud, 1, True, True,
                   soffit=pre != "key")
    # The jamb.
    oo = (TEMPLATE_X, 40.0, 0.0)
    lo = (oo[0] - V_PROUD, oo[1] - RING, oo[2])
    hi = (oo[0] + SOFFIT, oo[1], oo[2] + J + JAMB_BURY - RING_JOINT)
    highs.append(block_object("tp_jamb", lo, hi, 0.028, mats["ring"], (lo[0] + 11.0, lo[1] - 4.0, lo[2] + 2.0)))
    jamb_stone(rb, JambMap(1, oo), isl, False)
    # The coping segment and its right-hand neighbour (the joint AO); the left end is the baked end cap.
    oo = (TEMPLATE_X, 50.0, 0.0)
    lc = isl["cop_lfront"].w
    for j, du in enumerate((0.0, lc + 0.01)):
        y_hi, y_lo = oo[1] - du, oo[1] - du - lc
        highs.append(block_object(f"tp_copl{j}", (oo[0] - LOW_OUT, y_lo + 0.005, oo[2]),
                                  (oo[0] + CAP_BACK - 0.04, y_hi - 0.005, oo[2] + LOW_H - 0.004), 0.015, grey,
                                  (oo[0] + 3.0 * j, y_lo - 9.0, oo[2] + 4.0)))
        highs.append(block_object(f"tp_copc{j}", (oo[0] - CAP_OUT, y_lo + 0.005, oo[2] + LOW_H),
                                  (oo[0] + CAP_BACK, y_hi - 0.005, oo[2] + TOP - FACE_TOP), 0.02, grey,
                                  (oo[0] - 6.0 * j, y_lo + 13.0, oo[2] - 3.0)))
    cm = CopingMap(0.0, oo)
    coping_segment(rb, cm, isl, lc, (True, False))
    receiver = rb.obj("tp_receiver", [bpy.data.materials.new(f"tp_recv_{n}") for n in SLOTS])
    return highs, receiver


def seam_errors(px, isl, size, name="panel"):
    """Wrap check on a stone strip island: the gutter beyond each wrapping edge against the interior
    strip at the opposite edge (mean absolute difference per channel). Every strip wraps along the
    wall (u); only the panel wraps up the wall (v)."""
    i = isl[name]
    img = px.reshape(size, size, -1)
    f = size / ATLAS
    x0, y0 = int((i.x0 + GUT) * f), int((i.y0 + GUT) * f)
    pw, ph = int(round(PW * TEXEL * f)), int(round(i.h * TEXEL * f))
    g = max(2, int(GUT * f) - 1)
    right = img[y0:y0 + ph, x0 + pw:x0 + pw + g]
    left_in = img[y0:y0 + ph, x0:x0 + g]
    out = {"u": float(np.abs(right - left_in)[..., :3].mean())}
    if name == "panel":
        top = img[y0 + ph:y0 + ph + g, x0:x0 + pw]
        bot_in = img[y0:y0 + g, x0:x0 + pw]
        out["v"] = float(np.abs(top - bot_in)[..., :3].mean())
    return out


# ---------------------------------------------------------------------------------------------
# The LODs
def panel_rows(top, sc, bottom=-BURY):
    """Rows of panel cells (row index, h0, h1) from `bottom` up to `top`, each PH * sc tall; row -1
    holds the buried strip."""
    rows, r = [], -1
    while r * PH * sc < top:
        h0, h1 = max(r * PH * sc, bottom), min((r + 1) * PH * sc, top)
        if h1 > h0 + 1e-6:
            rows.append((r, h0, h1))
        r += 1
    return rows


def fringe_positions():
    """The fringe cards' s along the coping, at irregular steps from -FRINGE_S to FRINGE_S."""
    rnd, s, out = random.Random("fringe"), -FRINGE_S, []
    while s <= FRINGE_S:
        out.append(s)
        s += rnd.uniform(*FRINGE_STEP)
    return out


def build_lod(spec, lod, isl, mats):
    d = LOD[lod]
    b = Builder(SLOTS)
    park = {n: isl[f"park_{n}"].uv(0.5, 0.5) for n in ("turf", "iron", "glass")}
    X = (-1.0, 0.0, 0.0)
    sc = d["scale"]

    # The face: panel cells in rows of PH (row -1 holds the buried strip), each row's cells shifted,
    # cut along the ring's top near the arch.
    # Rows (island, h0, h1, the row's island origin hc). At true scale: the buried strip on the panel,
    # the foot band, two panel rows, the top band under the coping. LOD2 maps the panel alone at its
    # scale (only LOD0's density is gated).
    if sc == 1.0:
        rows = [("panel", -BURY, 0.0, -PH), ("band_foot", 0.0, CH, 0.0), ("panel", CH, CH + PH, CH),
                ("panel", CH + PH, CH + 2 * PH, CH + PH), ("band_top", CH + 2 * PH, FACE_TOP, CH + 2 * PH)]
        assert abs(CH + 2 * PH + CH - FACE_TOP) < 1e-9
    else:
        rows = [("panel", h0, h1, r * PH * sc) for r, h0, h1 in panel_rows(FACE_TOP, sc)]
    for ri, (name, h0, h1, hc) in enumerate(rows):
        s_c = W + (0.83 * (ri + 1)) % PW * sc
        while s_c - PW * sc > W:
            s_c -= PW * sc
        while s_c > -W:
            sa, sb = max(s_c - PW * sc, -W), min(s_c, W)
            if sb > sa + 1e-6:
                fm = FaceMap(s_c, hc)
                u_lo, u_hi = s_c - sb, s_c - sa
                v0, v1 = h0 - hc, h1 - hc
                touches = not (sb <= -(A + RING) - 0.01 or sa >= A + RING + 0.01)
                if not touches:
                    panel_cell(b, fm, isl, u_lo, u_hi, v0, v1, scale=sc, expect=X, name=name)
                elif d["strip"] is not None:
                    k = max(1, math.ceil((u_hi - u_lo) / d["strip"]))
                    for q in range(k):
                        ua, ub = u_lo + (u_hi - u_lo) * q / k, u_lo + (u_hi - u_lo) * (q + 1) / k
                        panel_cell(b, fm, isl, ua, ub, v0, v1, bottom=lambda u, fm=fm, hc=hc: keepout(fm.s_c - u) - hc,
                                   scale=sc, expect=X, name=name)
                else:
                    for lo_u, hi_u in ((u_lo, min(u_hi, s_c - (A + RING))), (max(u_lo, s_c + (A + RING)), u_hi)):
                        if hi_u > lo_u + 1e-6:
                            panel_cell(b, fm, isl, lo_u, hi_u, v0, v1, scale=sc, expect=X, name=name)
                    # the spandrels above the ring, one cut cell each side
                    for a_s, b_s in ((A + RING, KEY_HALF), (-KEY_HALF, -(A + RING))):
                        ua, ub = max(u_lo, s_c - a_s), min(u_hi, s_c - b_s)
                        if ub > ua + 1e-6:
                            panel_cell(b, fm, isl, ua, ub, v0, v1, bottom=lambda u, fm=fm, hc=hc: keepout(fm.s_c - u) - hc,
                                       scale=sc, expect=X, name=name)
            s_c -= PW * sc

    # The ring: voussoirs alternating v0 / v1, the keystone at the crown.
    ths = ellipse_divisions(d["segs"])
    nv = d["segs"]
    g = RING_JOINT / 2 / math.hypot(A, B) * 2
    for i in range(nv):
        t0, t1 = ths[i] + (g if i else 0.0), ths[i + 1] - (g if i < nv - 1 else 0.0)
        key = i == nv // 2
        am = ArchMap(t0, t1)
        if key:
            ring_stone(b, am, isl, "key", 2 * KEY_HALF, RING + KEY_UP, KEY_PROUD, d["sub"], d["sides"], d["edges"],
                       "v0_soffit", soffit=d["edges"])
        else:
            ring_stone(b, am, isl, "v0" if i % 2 == 0 else "v1", 0.75, RING, V_PROUD, d["sub"], d["sides"], d["edges"],
                       soffit=d["edges"])
    # The jambs (the s < 0 one mirrored on the same island).
    for side in (1, -1):
        jm = JambMap(side)
        if d["edges"]:
            jamb_stone(b, jm, isl, side < 0)
        else:
            top = J + JAMB_BURY - RING_JOINT
            q = [(0.0, 0.0), (RING, 0.0), (RING, top), (0.0, top)]
            m = (lambda u: RING - u) if side < 0 else (lambda u: u)
            b.face([jm(u, v, V_PROUD) for u, v in q], [isl["jamb_front"].uv(m(u), v) for u, v in q], "stone", X)

    # The lining from behind the ring stones to the back plane, and the back plane.
    prof_t = ellipse_divisions(d["segs"] * d["sub"])
    prof = [(A, -BURY)] + [intrados(t) for t in prof_t] + [(-A, -BURY)]
    cum = [0.0]
    for (s0, h0), (s1, h1) in zip(prof, prof[1:]):
        cum.append(cum[-1] + math.hypot(s1 - s0, h1 - h0))
    li = isl["lining"]
    xa = X_F + SOFFIT
    # LOD0 and LOD1: BARREL m of coursed stone first (the barrel island wraps every two courses
    # round the girth, so each profile segment splits there), then the lining.
    xl = xa + BARREL if lod < 2 else xa
    if lod < 2:
        bi, period = isl["barrel"], BARREL_COURSES * CH
        for k in range(len(prof) - 1):
            (s0, h0), (s1, h1) = prof[k], prof[k + 1]
            hm, sm = (h0 + h1) / 2, (s0 + s1) / 2
            inward = (0.0, -sm, (J - hm) if hm > J else 0.0)
            g0, g1 = cum[k], cum[k + 1]
            cuts = [g0] + [period * m for m in range(math.floor(g0 / period) + 1, math.ceil(g1 / period))] + [g1]
            for ga, gb in zip(cuts, cuts[1:]):
                ta, tb = (ga - g0) / (g1 - g0), (gb - g0) / (g1 - g0)
                pa = (s0 + (s1 - s0) * ta, h0 + (h1 - h0) * ta)
                pb = (s0 + (s1 - s0) * tb, h0 + (h1 - h0) * tb)
                base = period * math.floor((ga + gb) / 2 / period)
                va, vb = ga - base, gb - base
                b.face([(xa, *pa), (xl, *pa), (xl, *pb), (xa, *pb)],
                       [bi.uv(0.0, va), bi.uv(BARREL, va), bi.uv(BARREL, vb), bi.uv(0.0, vb)], "stone", inward)
    for k in range(len(prof) - 1):
        (s0, h0), (s1, h1) = prof[k], prof[k + 1]
        f0, f1 = cum[k] / cum[-1], cum[k + 1] / cum[-1]
        hm, sm = (h0 + h1) / 2, (s0 + s1) / 2
        inward = (0.0, -sm, (J - hm) if hm > J else 0.0)
        b.face([(xl, s0, h0), (X_B, s0, h0), (X_B, s1, h1), (xl, s1, h1)],
               [li.uv(f0, 0.0), li.uv(f0, 1.0), li.uv(f1, 1.0), li.uv(f1, 0.0)], "lining", inward)
    bk = isl["back"]
    cen = (X_B, 0.0, (J - BURY) / 2)
    cuv = bk.uv(0.5, 0.5)
    for k in range(len(prof) - 1):
        (s0, h0), (s1, h1) = prof[k], prof[k + 1]
        b.face([cen, (X_B, s0, h0), (X_B, s1, h1)],
               [cuv, bk.uv(0.5 - s0 / (2 * A + 1), (h0 + BURY) / 4.0), bk.uv(0.5 - s1 / (2 * A + 1), (h1 + BURY) / 4.0)],
               "back", X)
    b.face([(X_B, -A, -BURY), (X_B, A, -BURY), cen], [bk.uv(0.95, 0.0), bk.uv(0.05, 0.0), cuv], "back", X)

    # The coping, the turf behind it, the wall ends and the back.
    n = d["coping"]
    lc = 2 * W / n
    for i in range(n):
        s0 = W - i * lc
        coping_segment(b, CopingMap(s0), isl, lc, (i == 0, i == n - 1))
    b.face([(X_F + CAP_BACK, -W, TOP), (X_E, -W, TOP), (X_E, W, TOP), (X_F + CAP_BACK, W, TOP)], [park["turf"]] * 4,
           "turf", (0.0, 0.0, 1.0))
    for side in (1, -1):
        # u to the viewer's right: -x seen from +y, +x seen from -y.
        def end_map(u, v, side=side):
            x = X_E - u if side > 0 else X_F + u
            return (x, side * W, END_B + v)
        end_map_obj = end_map
        cells = [(0.0, X_E - X_F)] if X_E - X_F <= PW * sc else [(0.0, PW * sc), (PW * sc, X_E - X_F)]
        for ua, ub in cells:
            panel_cell(b, end_map_obj, isl, ua, ub, 0.0, TOP - END_B, scale=sc, expect=(0.0, side, 0.0))
    # The back face (seen from +x, the viewer's right is +s), rows of panel cells.
    for r, h0, h1 in panel_rows(TOP, sc):
        hc = r * PH * sc
        s_c = -W
        while s_c < W:
            sb = min(s_c + PW * sc, W)

            def back_map(u, v, s_c=s_c, hc=hc):
                return (X_E, s_c + u, hc + v)
            panel_cell(b, back_map, isl, 0.0, sb - s_c, h0 - hc, h1 - hc, scale=sc, expect=(1.0, 0.0, 0.0))
            s_c = sb

    # The lanterns.
    for side in (1, -1):
        s0 = side * LAMP_S
        cx = X_F - ARM_OUT + 0.02
        hw = LAMP_HW
        top_w = hw * 1.18

        def quad_ring(z0, z1, w0, w1, slot, uv):
            p0 = [(cx - w0, s0 - w0), (cx + w0, s0 - w0), (cx + w0, s0 + w0), (cx - w0, s0 + w0)]
            p1 = [(cx - w1, s0 - w1), (cx + w1, s0 - w1), (cx + w1, s0 + w1), (cx - w1, s0 + w1)]
            for i in range(4):
                a0, b0, a1, b1 = p0[i], p0[(i + 1) % 4], p1[i], p1[(i + 1) % 4]
                mid = ((a0[0] + b0[0]) / 2 - cx, (a0[1] + b0[1]) / 2 - s0, 0.0)
                b.face([(a0[0], a0[1], z0), (b0[0], b0[1], z0), (b1[0], b1[1], z1), (a1[0], a1[1], z1)], [uv] * 4, slot,
                       mid)

        quad_ring(LAMP_Z0, LAMP_Z1, hw, top_w, "glass", park["glass"])
        apex = (cx, s0, LAMP_Z1 + 0.11)
        cap = [(cx - top_w - 0.02, s0 - top_w - 0.02), (cx + top_w + 0.02, s0 - top_w - 0.02),
               (cx + top_w + 0.02, s0 + top_w + 0.02), (cx - top_w - 0.02, s0 + top_w + 0.02)]
        for i in range(4):
            a, c_ = cap[i], cap[(i + 1) % 4]
            mid = ((a[0] + c_[0]) / 2 - cx, (a[1] + c_[1]) / 2 - s0, 0.5)
            b.face([(a[0], a[1], LAMP_Z1), (c_[0], c_[1], LAMP_Z1), apex], [park["iron"]] * 3, "iron", mid)
        if d["lamp"] >= 1:
            b.face([(p[0], p[1], LAMP_Z1) for p in cap[::-1]], [park["iron"]] * 4, "iron", (0.0, 0.0, -1.0))
            base = [(cx - hw, s0 - hw), (cx + hw, s0 - hw), (cx + hw, s0 + hw), (cx - hw, s0 + hw)]
            tip = (cx, s0, LAMP_Z0 - 0.07)
            for i in range(4):
                a, c_ = base[i], base[(i + 1) % 4]
                mid = ((a[0] + c_[0]) / 2 - cx, (a[1] + c_[1]) / 2 - s0, -0.5)
                b.face([(c_[0], c_[1], LAMP_Z0), (a[0], a[1], LAMP_Z0), tip], [park["iron"]] * 3, "iron", mid)
        # The arm: a square iron bar from the wall to above the lantern, then a curl down at its tip.
        bw = 0.013
        path = [(X_F - 0.005, ARM_Z), (X_F - ARM_OUT * 0.5, ARM_Z + 0.015), (cx, ARM_Z)] if d["lamp"] else \
            [(X_F - 0.005, ARM_Z), (cx, ARM_Z)]
        if d["lamp"] >= 1:
            path += [(cx - 0.05, ARM_Z - 0.02), (cx - 0.065, ARM_Z - 0.06), (cx - 0.045, ARM_Z - 0.095),
                     (cx - 0.012, ARM_Z - 0.09)]
        braces = []
        if d["lamp"] == 2:
            # a scroll brace under the arm
            br = []
            for q in range(9):
                a = q / 8
                br.append((X_F - 0.005 - a * (ARM_OUT * 0.62), ARM_Z - 0.26 + a * 0.24 + 0.05 * math.sin(a * math.pi)))
            cxs, czs = br[-1]
            for q in range(1, 6):
                ang = q / 5 * 1.6 * math.pi
                br.append((cxs - 0.035 * math.sin(ang), czs + 0.035 * (1 - math.cos(ang)) - 0.0))
            braces.append(br)
        for pth in [path] + braces:
            for (xa_, za), (xb_, zb) in zip(pth, pth[1:]):
                for (sa, sb_, za_off, zb_off, nrm) in ((-bw, bw, bw, bw, (0.0, 0.0, 1.0)), (-bw, bw, -bw, -bw, (0.0, 0.0, -1.0)),
                                                       (bw, bw, -bw, bw, (0.0, 1.0, 0.0)), (-bw, -bw, -bw, bw, (0.0, -1.0, 0.0))):
                    if za_off == zb_off:
                        pts = [(xa_, s0 + sa, za + za_off), (xb_, s0 + sa, zb + za_off), (xb_, s0 + sb_, zb + za_off),
                               (xa_, s0 + sb_, za + za_off)]
                    else:
                        pts = [(xa_, s0 + sa, za + za_off), (xb_, s0 + sa, zb + za_off), (xb_, s0 + sa, zb + zb_off),
                               (xa_, s0 + sa, za + zb_off)]
                    b.face(pts, [park["iron"]] * 4, "iron", nrm)
        if d["lamp"]:  # the hanger: a short rod from the arm to the cap's apex
            b.face([(cx - 0.008, s0, apex[2]), (cx + 0.008, s0, apex[2]), (cx + 0.008, s0, ARM_Z - 0.01),
                    (cx - 0.008, s0, ARM_Z - 0.01)], [park["iron"]] * 4, "iron", X)

    # Fern clumps: fronds fanning out of one root, over the coping's front (they arch out and hang
    # down the face) or on the bank at the wall's foot (they arch up and out).
    hw = FERN_W / 2
    for i, (fs, where) in enumerate(FERN_CLUMPS):
        if lod == 2 and i % LOD2_CLUMP_STEP:
            continue
        rnd = random.Random(f"fern-{i}")
        sc, yaw = rnd.uniform(*FERN_SCALE), rnd.uniform(-FERN_YAW, FERN_YAW)
        lean = rnd.uniform(*FERN_LEAN)
        if where == "coping":
            root = (X_F + 0.10, fs, TOP + 0.02)
            spread, rise, droop = 2.2, 0.20 * lean, 0.72 / lean
        else:
            x0 = X_F - 0.22
            root = (x0, fs, cut_height_at(x0, -fs) - 0.05)
            spread, rise, droop = 2.6, 0.42 * lean, 0.30 / lean
        nf = d["fronds"]
        for j in range(nf):
            f = (j + 0.5) / nf - 0.5 if nf > 1 else rnd.uniform(-0.15, 0.15)
            ang = math.pi + yaw + f * spread + rnd.uniform(-0.15, 0.15)  # pi: out over the face (-x)
            cosa, sina = math.cos(ang), math.sin(ang)
            ln = FERN_L * sc * rnd.uniform(0.85, 1.0)
            hw = FERN_W * sc / 2
            # (u along the frond, horizontal reach, height): up out of the root, then over and down.
            path = [(0.0, 0.0, 0.0), (0.5, 0.42 * ln, rise * ln), (1.0, 0.84 * ln, (rise - droop) * ln)]
            if d["frond_segs"] == 1:
                path = [path[0], path[2]]
            k = (i + j) % FERN_VARIANTS
            fi = isl[f"fern{k}"]

            def pt(r, z, w, cosa=cosa, sina=sina):
                return (root[0] + r * cosa - w * sina, root[1] + r * sina + w * cosa, root[2] + z)
            for (ua, ra, za), (ub, rb, zb) in zip(path, path[1:]):
                pts = [pt(ra, za, -hw), pt(rb, zb, -hw), pt(rb, zb, hw), pt(ra, za, hw)]
                uvs = [fi.uv(ua * FERN_L, 0.0), fi.uv(ub * FERN_L, 0.0), fi.uv(ub * FERN_L, FERN_W),
                       fi.uv(ua * FERN_L, FERN_W)]
                b.face(pts, uvs, "fern", (0.0, 0.0, 1.0))
    # The grass fringe over the coping's lip: one card per step along the cap top's front, blades
    # out and down over the coping (every LOD's "tufts"-th card).
    for i, ts in enumerate(fringe_positions()):
        if i % d["tufts"]:
            continue
        rnd = random.Random(f"tuft-{i}")
        ti = isl[f"tuft{i % TUFT_VARIANTS}"]
        root = (X_F - CAP_OUT + 0.05, ts, TOP + 0.01)
        yaw = rnd.uniform(-0.3, 0.3)
        cy, sy = math.cos(yaw), math.sin(yaw)
        half = TUFT / 2
        ln = rnd.uniform(*FRINGE_LEN)
        out, down = TUFT * 0.36 * ln, TUFT * 0.93 * ln

        def p(w, t, cy=cy, sy=sy, root=root, out=out, down=down):  # w across, t from root (0) to tip (1)
            return (root[0] - t * out * cy - w * sy, root[1] - t * out * sy + w * cy, root[2] - t * down)
        pts = [p(-half, 0.0), p(half, 0.0), p(half, 1.0), p(-half, 1.0)]
        uvs = [ti.uv(0.0, 0.0), ti.uv(TUFT, 0.0), ti.uv(TUFT, TUFT), ti.uv(0.0, TUFT)]
        b.face(pts, uvs, "fern", (-1.0, 0.0, 0.3))

    o = b.obj(f"{spec['piece']}_lod{lod}", mats)
    return o


def slot_density(o, slot_index):
    """Per-face px/m of one material on UV0 against the atlas, as the gate measures it."""
    me = o.data
    uvl = me.uv_layers["UVMap"].data
    vals = []
    for p in me.polygons:
        if p.material_index != slot_index or p.area <= 1e-9:
            continue
        uvs = [uvl[li].uv for li in p.loop_indices]
        a = 0.0
        for i in range(1, len(uvs) - 1):
            e1, e2 = uvs[i] - uvs[0], uvs[i + 1] - uvs[0]
            a += abs(e1.x * e2.y - e1.y * e2.x) / 2
        vals.append(math.sqrt(a * ATLAS * ATLAS / p.area))
    vals.sort()
    return {"min": vals[0], "median": vals[len(vals) // 2], "max": vals[-1], "faces": len(vals)}


# ---------------------------------------------------------------------------------------------
def _fill_box(img, box, value):
    x0, y0, x1, y1 = box
    img[y0:y1, x0:x1] = value


def compose_maps(isl, baked, spec):
    """The shipped maps from the baked stone islands (or flat stand-ins) plus the numpy regions."""
    n = ATLAS
    # The albedo arrays hold sRGB-encoded values (the EMIT bake reloaded Non-Color, as _boulder does),
    # so the background and the fern tiles go in encoded.
    stone_mean = np.array((0x9a / 255, 0x96 / 255, 0x8c / 255, 1.0), np.float32)
    alb = np.tile(stone_mean, (n, n, 1)).astype(np.float32)
    nrm = np.tile(np.array((0.5, 0.5, 1.0, 1.0), np.float32), (n, n, 1))
    orm = np.tile(np.array((1.0, 0.85, 0.0, 1.0), np.float32), (n, n, 1))
    if baked is not None:
        cov = baked["cover"]
        alb[cov] = baked["albedo"][cov]
        nrm[cov] = baked["normal"][cov]
        orm[cov, 0] = baked["ao"][cov, 0]
        orm[cov, 1] = baked["rough"][cov, 0]
    # Ferns: _ferns tiles drawn at the island's pixel size (u along the frond).
    for k in range(FERN_VARIANTS):
        fi = isl[f"fern{k}"]
        x0, y0, x1, y1 = fi.px_box()
        a, nn, r, ao = _ferns.frond_tile(x1 - x0, y1 - y0, "road_tunnel_portal", k)
        alb[y0:y1, x0:x1, :3] = a[..., :3]
        alb[y0:y1, x0:x1, 3] = a[..., 3]
        nrm[y0:y1, x0:x1, :3] = nn
        orm[y0:y1, x0:x1, 0] = ao
        orm[y0:y1, x0:x1, 1] = np.maximum(r, 0.85)
    # Grass tufts: _props side-view tuft tiles (roots along the bottom row), square.
    for k in range(TUFT_VARIANTS):
        x0, y0, x1, y1 = isl[f"tuft{k}"].px_box()
        a, nn, r = _props.tuft_tile(x1 - x0, None, "road_tunnel_portal", k)
        alb[y0:y1, x0:x1] = a
        nrm[y0:y1, x0:x1, :3] = nn
        orm[y0:y1, x0:x1, 0] = 1.0
        orm[y0:y1, x0:x1, 1] = np.maximum(r, 0.85)
    # Masks: stone values everywhere, then each region's hooks.
    msk = np.tile(np.array((1.0, 0.42, 0.0, 1.0), np.float32), (n, n, 1))
    for name in [f"fern{k}" for k in range(FERN_VARIANTS)] + [f"tuft{k}" for k in range(TUFT_VARIANTS)]:
        _fill_box(msk, isl[name].px_box(), (0.5, 0.6, 0.0, 1.0))
    _fill_box(msk, isl["park_turf"].px_box(), (1.0, 0.6, 0.0, 1.0))
    _fill_box(msk, isl["park_iron"].px_box(), (0.5, 0.15, 0.0, 1.0))
    _fill_box(msk, isl["park_glass"].px_box(), (0.0, 0.1, 1.0, 1.0))
    _fill_box(msk, isl["back"].px_box(), (0.1, 0.3, 0.0, 0.12))
    # The barrel: the panel's two lower courses (v wraps at the joint every two courses), faded with
    # depth u in albedo (linear), AO and the masks' AO down to the lining's start.
    bi, pi_ = isl["barrel"], isl["panel"]
    bw, bh = round(BARREL * TEXEL), round(BARREL_COURSES * CH * TEXEL)
    cols = np.arange(-GUT, bw + GUT)
    rows = np.mod(np.arange(-GUT, bh + GUT), bh)
    src_x = pi_.x0 + GUT + 100 + cols
    src_y = pi_.y0 + GUT + rows
    dst = (slice(bi.y0, bi.y0 + bh + 2 * GUT), slice(bi.x0, bi.x0 + bw + 2 * GUT))
    u = np.clip((cols + 0.5) / bw, 0.0, 1.0)
    ramp = u * u * (3 - 2 * u)
    g_alb = 1.0 - (1.0 - BARREL_ALBEDO_END) * ramp
    g_ao = 1.0 - (1.0 - BARREL_AO_END) * ramp
    patch = alb[np.ix_(src_y, src_x)].copy()
    patch[..., :3] = np.power(np.power(np.clip(patch[..., :3], 0.0, 1.0), 2.2) * g_alb[None, :, None], 1 / 2.2)
    alb[dst] = patch
    nrm[dst] = nrm[np.ix_(src_y, src_x)]
    o = orm[np.ix_(src_y, src_x)].copy()
    o[..., 0] *= g_ao[None, :]
    orm[dst] = o
    msk[dst[0], dst[1], 0] = 0.15
    msk[dst[0], dst[1], 1] = 0.35
    msk[dst[0], dst[1], 2] = 0.0
    msk[dst[0], dst[1], 3] = g_ao[None, :]
    li = isl["lining"]
    x0, y0, x1, y1 = li.px_box()
    q = np.clip((np.arange(y0, y1) + 0.5 - (li.y0 + GUT)) / li.ph, 0.0, 1.0)
    a_col = BARREL_AO_END - (BARREL_AO_END - 0.12) * (q * q * (3 - 2 * q))
    msk[y0:y1, x0:x1, 0] = 0.15
    msk[y0:y1, x0:x1, 1] = 0.35
    msk[y0:y1, x0:x1, 2] = 0.0
    msk[y0:y1, x0:x1, 3] = a_col[:, None]
    return alb, nrm, orm, msk


def _down(px, size, out, renormal=False):
    return _rock._downsample(px.reshape(-1, 4), size, out, renormal=renormal)


def build_piece(spec, generator):
    piece = spec["piece"]
    out = pathlib.Path(sys.argv[sys.argv.index("--") + 1])
    skip_bake = os.environ.get("HEXW_SKIP_BAKE") == "1"
    scene.reset_scene(piece)
    field = check_field()
    print(f"{piece}: field matches hex-core slope at {field}")
    isl = pack(_islands())
    mats = [materials.pbr_material(f"{piece}_{n}", (*spec["colors"][n], 1.0), spec["rough"][n]) for n in SLOTS]
    highs, receiver = build_templates(isl, stone_materials(piece))
    signature = bake.receiver_signature([receiver])
    stats = {}
    baked = None
    if not skip_bake:
        scn = bpy.context.scene
        scn.world = scn.world or bpy.data.worlds.new(piece)
        scn.world.light_settings.distance = AO_DIST
        bake.setup_cycles(samples=SAMPLES)
        kw = dict(cage_extrusion=CAGE, max_ray=MAX_RAY, margin=MARGIN)
        imgs = {k: bake.new_image(f"{piece}.{k}", ATLAS, non_color=k != "albedo") for k in ("albedo", "normal", "ao", "rough")}
        bake.bake_emit_input(highs, receiver, "Base Color", imgs["albedo"], **kw)
        bake.bake_to(receiver, "NORMAL", imgs["normal"], highs=highs, **kw)
        bake.bake_to(receiver, "ROUGHNESS", imgs["rough"], highs=highs, **kw)
        bake.bake_to(receiver, "AO", imgs["ao"], highs=highs, **kw)
        px = {}
        for k, im in imgs.items():
            p = bake.save_png(im, str(out / f"{piece}.{k}.tmp.png"))
            px[k] = bake.load_png(p)[1].reshape(ATLAS, ATLAS, 4)
            os.remove(p)
        cover = np.zeros((ATLAS, ATLAS), bool)
        for name in isl:
            if name.split("_")[0] in ("panel", "band", "v0", "v1", "key", "jamb", "cop"):
                x0, y0, x1, y1 = isl[name].px_box()
                cover[y0:y1, x0:x1] = True
        px["cover"] = cover
        baked = px
        ins = cover.copy()
        stats["normalMissFraction"] = float((px["normal"][ins, 2] < 0.05).mean())
        a = px["ao"][ins, 0]
        stats["ao"] = {"mean": float(a.mean()), "p05": float(np.percentile(a, 5))}
        stats["albedoLuma"] = float(px["albedo"][ins, :3].mean())
        stats["seam"] = {f"{s}.{k}": seam_errors(px[k], isl, ATLAS, s) for s in ("panel", "band_top", "band_foot")
                         for k in ("albedo", "normal", "ao")}
        print(f"{piece}: bake stats {json.dumps(stats)}")
        bad = {k: v for k, v in stats["seam"].items() if max(v.values()) > (0.06 if k.endswith("ao") else 0.02)}
        assert not bad, f"a stone strip island does not wrap: {bad}"
    alb, nrm, orm, msk = compose_maps(isl, baked, spec)
    bake.write_png(ATLAS, alb.reshape(-1, 4), str(out / f"{piece}.albedo.png"), non_color=False)
    bake.write_png(NORMAL_SIZE, _down(nrm, ATLAS, NORMAL_SIZE, renormal=True), str(out / f"{piece}.normal.png"))
    bake.write_png(ORM_SIZE, _down(orm, ATLAS, ORM_SIZE), str(out / f"{piece}.orm.png"))
    bake.write_png(MASK_SIZE, _down(msk, ATLAS, MASK_SIZE), str(out / f"{piece}.masks.png"))
    for o in list(highs) + [receiver]:
        bpy.data.objects.remove(o, do_unlink=True)
    bpy.context.view_layer.update()

    root = scene.empty(piece)
    lods = [build_lod(spec, i, isl, mats) for i in range(3)]
    tris = [len(o.data.polygons) for o in lods]
    density = {n: slot_density(lods[0], SLOTS.index(n)) for n in ("stone", "fern")}
    print(f"{piece}: tris per LOD {tris}, density {density}")
    budgets.check(tris, "prop", piece)
    for n, v in density.items():
        assert v["median"] >= 256.0, (n, v)
    # The collider: the wall body's box (LOD2's stone faces), so lanterns and ferns stay out of it.
    tmp = lods[2].copy()
    tmp.data = lods[2].data.copy()
    scene.link(tmp)
    bm = bmesh.new()
    bm.from_mesh(tmp.data)
    bmesh.ops.delete(bm, geom=[f for f in bm.faces if f.material_index != SLOTS.index("stone")], context="FACES")
    bm.to_mesh(tmp.data)
    bm.free()
    col = collider.collider_from(tmp, "box", f"{piece}_collider")
    bpy.data.objects.remove(tmp, do_unlink=True)
    for o in lods + [col]:
        o.parent = root
    export.export_glb(root, str(out / f"{piece}.glb"))
    export.write_export_report(str(out / "export-report.json"), root, piece, generator)
    # kit-compress ships the PNG fallback albedo at PNG_ALBEDO (owner O1, 2026-10-29): the KTX2
    # families keep the 1024² atlas, and the png download family stays under its 30 MiB budget.
    rep_path = out / "export-report.json"
    rep = json.loads(rep_path.read_text())
    rep["pngFallbackSize"] = {"albedo": PNG_ALBEDO}
    rep_path.write_text(json.dumps(rep, indent=2, sort_keys=True) + "\n")
    xs = [v.co.x for v in col.data.vertices]
    bake.write_bake_report(
        str(out / "bake-report.json"), receiverSignature=signature, device=os.environ.get("HEXW_BAKE_DEVICE", "CPU"),
        samples=SAMPLES, marginPx=MARGIN, cageExtrusionM=CAGE, maxRayDistanceM=MAX_RAY, aoDistanceM=AO_DIST,
        channels={"albedo": {"size": ATLAS, "colorspace": "srgb"}, "normal": {"size": NORMAL_SIZE, "colorspace": "linear"},
                  "orm": {"size": ORM_SIZE, "colorspace": "linear"}, "masks": {"size": MASK_SIZE, "colorspace": "linear"}},
        texelDensityPxPerM=density, bakeStats=stats, lodTris=tris, lod2Drops={},
        islands={k: [v.x0, v.y0, v.pw, v.ph] for k, v in sorted(isl.items())},
        portal={"field": "cut", "fieldCheck": field, "xFace": round(X_F, 4), "xBack": round(X_B, 4), "wallTop": TOP, "crown": round(CROWN, 4),
                "openingHalfSpan": A, "springing": J, "colliderX": [round(min(xs), 3), round(max(xs), 3)]},
        skipped=skip_bake)
    return root, lods
