"""_massif: the pipeline of massif_granite, the 7-cell mega-tile ground piece carrying a craggy granite peak of about 36 m
(owner A53 2026-10-29, "ref-2 approved": assets-source/refs/massif_granite/ref-2.jpg; ref-4 guides
the silhouette). Scripted route, kind `hero` (LOD0 <= 12,000 tris, KTX2 class hero), pack hero.

Geometry contract (mega-tiles plan, Global Constraints, the geometry pin):
- Footprint: the anchor (pivot, the centre cell's centre) plus `anchor.neighbor(0..6)`; ring cell i
  sits at 2 * APOTHEM * hexgeo.edge_normal(i) in Blender XY (runtime +z is Blender -y).
- Outer edge j in 0..18 is ring cell j // 3, local edge (j // 3 + 5 + j % 3) % 6; a cell's edge k
  runs from its corner k - 1 to its corner k (hexgeo.border_ring). The cap border is that 18-edge
  flower outline at z = 0 with BORDER_SUBDIV points per edge, the points a level-0 meadow_base
  beside each outer edge has on it, identical in every LOD, normals +Z. No interior edges.
- Side walls: the O4 soil skirt to z = -LEVEL_STEP, one row.

Surface: a heightfield ground cap plus a rock mesh planted in it.
- Ground: meadow_base's grass set and hollows (`_terrain.undulation`/`hollow`, MEADOW_DEPTH), with
  the falloff taken against the flower outline, and meadow's shading normals (the hollows' slope
  without the falloff's rise), so the ring reads as the meadow beside it. Runtime grass grows on it
  (grass::grass_slots: tileable `grass`, not foliage; grass::style's default is meadow's). The
  grass rises from the rim (flat only on its outer metre or two) as a flank of the distance to
  the outline, smooth-maxed with the form pass's slopes (ribs under the spines), and buries the
  rock's lower part: the rock foot sits about 9 m up (owner A55 Q1 O1); scree fans issue from
  the clefts between the buttresses (owner A53 Q3 O1: sculpted landform). Cap faces the rock
  covers are left out.
- Rock (not a heightfield, after ref-2 and ref-4), built as the mass first and the slabs second:
  the mass is a summit block with lower tops and six ridge spines running down into the ring
  cells, cleft along the gully bearings (`mass`). It becomes a signed-distance volume between its
  top and BURY under the grass; slab facets (a plane per seed, cells stretched along the fall
  line, tilted and lifted a little) and joints along the facet borders are carved into its outer
  layer (`rock_volume`), and OpenVDB meshes it. Smoothing (weathered edges) and triplanar grain
  make the high poly. LOD0 is that high poly decimated, with its buried faces cut; LOD1 decimates
  LOD0 and keeps its UVs; LOD2 is the volume meshed coarse and decimated (the rock does not touch
  the border), each with the open rim pushed back under the ground. Loose boulders on the ring
  and the pines of ref-2 are runtime scatter, never baked (owner 2026-10-26, A54 Q2 O1).

Textures: the rock and the scree are a unique bake on one 2048 atlas. LOD0's rock and scree
islands fill the right three quarters; the rock's LOD1 shares LOD0's islands; the scree's LOD1
and the rock and scree of LOD2 bake onto their own blocks in the left column. The ground and skirt sample masks
only, through a planar block of the same UV0. masks: R snow, G wetness (meadow's swell formula on
the ground), A ground AO; the rock's AO is in ORM.R (masks.A = 1 there, the boulders' convention)."""
import math
import os
import pathlib
import sys

import bmesh
import bpy
import numpy as np
from mathutils import Vector

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
sys.path.insert(0, str(pathlib.Path(__file__).parent))
from lib import bake, budgets, collider, export, geom, hexgeo, lod, materials, scene

import _boulder
import _rock
import _terrain

PIECE = "massif_granite"
KIND = "hero"
# Scree slot texture route, for the owner's options table: "unique" bakes it with the rock;
# "gravel" leaves it to the world-space gravel set (masks only), so the rock gets the whole atlas.
SCREE = os.environ.get("MASSIF_SCREE", "unique")
assert SCREE in ("unique", "gravel"), SCREE

ATLAS, NORMAL_SIZE, ORM_SIZE, MASK_SIZE = 2048, 2048, 512, 1024
G = 0.25  # left column width of the atlas: ground block, LOD1 and LOD2 rock blocks
GS = 0.96 * G  # each block's used side, leaving bake-margin room between blocks
SAMPLES, MARGIN = 32, 8
# The rock's bake reach: LOD0 lies within 0.24 m (p99 0.11 m) of the high poly, and a wider cage
# starts rays inside the neighbouring slab across a joint. AO distance 1.2 m darkens the joints and
# the cleft walls, not whole clefts (2.5 m gave a rock AO mean of 0.75; the boulders use 0.5 m).
CAGE, MAX_RAY, AO_DIST = 0.15, 0.3, 1.2
# The scree LODs' and rock LOD2's bake reach (cage, ray): LOD1/LOD2 ground lies within 0.99 m of
# the ground high poly, rock LOD2 within 0.7 m of the rock's (2.5 m back from the high poly).
SCREE_CAGE, ROCK2_CAGE = (1.0, 2.0), (0.9, 2.2)
# Ground point spacing per LOD (steep, mid, gentle, flat ring) and the fan points' step, in m.
SPACING = ((0.9, 1.3, 1.9, 2.8), (1.4, 2.0, 2.9, 4.2), (4.2, 5.6, 7.6, None))
FAN_STEP = (1.4, 2.2, 5.0)
STEEP, MID = 0.9, 0.35  # gradient magnitudes splitting the spacing classes

C = hexgeo.APOTHEM
CENTRES = [(0.0, 0.0)] + [tuple(2.0 * C * c for c in hexgeo.edge_normal(i)) for i in range(6)]


def outline_segments():
    """The 18 outer edges (a, b) in Blender XY, in outer-index order (the geometry pin)."""
    segs = []
    for j in range(18):
        ring = j // 3
        edge = (ring + 5 + j % 3) % 6
        cx, cy = CENTRES[1 + ring]
        a, b = hexgeo.corner((edge - 1) % 6), hexgeo.corner(edge)
        segs.append(((cx + a[0], cy + a[1]), (cx + b[0], cy + b[1])))
    for j in range(18):
        assert math.dist(segs[j][1], segs[(j + 1) % 18][0]) < 1e-9, j
    return segs


SEGS = outline_segments()
RING = [(a[0] + (b[0] - a[0]) * s / hexgeo.BORDER_SUBDIV, a[1] + (b[1] - a[1]) * s / hexgeo.BORDER_SUBDIV)
        for a, b in SEGS for s in range(hexgeo.BORDER_SUBDIV)]
SPAN = 2.0 * max(max(abs(x), abs(y)) for x, y in RING) + 0.2


def outline_dist(x, y):
    """Signed distance to the flower outline (positive inside), numpy or scalar."""
    x, y = np.asarray(x, float), np.asarray(y, float)
    d = np.full(x.shape, np.inf)
    for (ax, ay), (bx, by) in SEGS:
        dx, dy = bx - ax, by - ay
        t = np.clip(((x - ax) * dx + (y - ay) * dy) / (dx * dx + dy * dy), 0.0, 1.0)
        d = np.minimum(d, np.hypot(x - ax - t * dx, y - ay - t * dy))
    inside = np.max([C - np.max([(x - cx) * n[0] + (y - cy) * n[1]
                                 for n in (hexgeo.edge_normal(k) for k in range(6))], axis=0)
                     for cx, cy in CENTRES], axis=0) >= -1e-6  # member borders sit at 0 for two cells
    return np.where(inside, d, -d)


# --- deterministic numpy noise ------------------------------------------------------------------

def _hash(ix, iy, seed):
    h = (ix * 374761393 + iy * 668265263 + seed * 1442695041) & 0xFFFFFFFF
    h = ((h ^ (h >> 13)) * 1274126177) & 0xFFFFFFFF
    return ((h ^ (h >> 16)) & 0xFFFF).astype(np.float64) / 65535.0


def vnoise(x, y, seed):
    """Value noise in [0, 1] on a unit lattice, C1 (smoothstep) interpolation."""
    fx, fy = np.floor(x), np.floor(y)
    ix, iy = fx.astype(np.int64), fy.astype(np.int64)
    tx, ty = x - fx, y - fy
    ux, uy = tx * tx * (3 - 2 * tx), ty * ty * (3 - 2 * ty)
    a, b = _hash(ix, iy, seed), _hash(ix + 1, iy, seed)
    c, d = _hash(ix, iy + 1, seed), _hash(ix + 1, iy + 1, seed)
    return (a + (b - a) * ux) * (1 - uy) + (c + (d - c) * ux) * uy


def fbm(x, y, scale, octaves, seed):
    s, amp, tot = 0.0, 1.0, 0.0
    for o in range(octaves):
        f = 2.0 ** o / scale
        s = s + amp * vnoise(x * f + 17.3 * o, y * f - 9.1 * o, seed + o)
        tot += amp
        amp *= 0.5
    return s / tot


def ridged(x, y, scale, octaves, seed):
    s, amp, tot = 0.0, 1.0, 0.0
    for o in range(octaves):
        f = 2.0 ** o / scale
        n = 1.0 - np.abs(2.0 * vnoise(x * f + 5.7 * o, y * f + 3.3 * o, seed + 31 * o) - 1.0)
        s = s + amp * n * n
        tot += amp
        amp *= 0.5
    return s / tot


def sstep(a, b, x):
    t = np.clip((x - a) / (b - a), 0.0, 1.0)
    return t * t * (3.0 - 2.0 * t)


def smax(a, b, k):
    kk = np.maximum(k, 1e-6)
    h = np.clip(0.5 + 0.5 * (a - b) / kk, 0.0, 1.0)
    return np.where(k > 1e-6, b + (a - b) * h + kk * h * (1.0 - h), np.maximum(a, b))



# --- the landform (Blender XY, metres) ----------------------------------------------------------
# Form pass (2026-10-29), after ref-2 at its fitted camera (docs/checkpoints/massif_granite/form.md):
# the mass first, the slabs carved into it second. The mass is a summit with a lower second top,
# and six ridge spines running from the summit down into the ring cells (bearing deg from the
# summit, distance m from the summit to where the spine sinks under the grass, crest half widths at
# the root and at the tip). Gullies between the spines hold the scree fans. The grass slopes rise
# from the flat ring to the rock foot (concave, steepest at the foot), with grass ribs below the
# spines and hollows below the gullies.
SX, SY = -2.0, -1.0
SUMMIT_H = 33.6
# (x, y, height, foot radius, profile power p, shoulder power q, x stretch): h * (1 - (d / r) ** q) ** p,
# q > 1 for a massive top with steep lower faces, not a cone.
TOPS = ((SX, SY, SUMMIT_H, 13.0, 1.0, 1.15, 1.18), (7.0, -2.5, 26.0, 8.0, 1.0, 1.15, 1.0),
        (-8.0, -5.0, 20.5, 6.5, 1.0, 1.1, 1.0), (-5.0, 5.5, 22.5, 6.5, 1.0, 1.1, 1.0))
CLEFT_DEPTH, CLEFT_SIGMA = 0.8, 0.30  # share of the tops' rise the gully clefts cut, their angular width (rad)
SPINES = ((-146.0, 19.5, 3.2, 2.2), (-86.0, 19.5, 3.2, 2.4), (-3.0, 18.5, 2.9, 2.2), (29.0, 25.0, 3.0, 2.0),
          (72.0, 18.0, 2.9, 2.0), (127.0, 25.0, 3.0, 2.2))
SPINE_E0 = 20.0  # crest height over the grass at the summit end (m)
# Ground: height A_G at R_IN from the summit, falling as ((R_OUT - r) / (R_OUT - R_IN)) ** P_G.
A_G, R_IN, R_OUT, P_G = 11.0, 6.0, 29.0, 1.6
APRON_R = 34.0  # reach of the ground's bake high poly
# Form pass 3 (owner A55 Q1 O1, 2026-10-29: taller grassy flanks as in ref-2; the rock itself stays
# where the form pass put it): only the grass rises, as a flank of the distance to the flower
# outline, 0 at all 18 outer edges and easing in over the first metres, FLANK_H by FLANK_D plus
# FLANK_H2 more by FLANK_D2, so the grass buries the rock's lower part and meets it about 9 m up.
# The distance is the outline's with its creases rounded over FLANK_SOFT m; the flank and the form
# pass's slopes join in a smooth max of reach FLANK_K.
FLANK_H, FLANK_D, FLANK_H2, FLANK_D2 = 7.5, 15.0, 2.5, 30.0
FLANK_SOFT, FLANK_K = 3.0, 2.0
# Scree fans in the gullies (bearing deg from the summit, half angle deg, length m): a lens up to
# FAN_T thick from the rock foot down the grass slope.
FANS = ((-116.0, 16.0, 16.0), (-46.0, 16.0, 15.0), (168.0, 16.0, 14.0), (12.0, 10.0, 9.0), (100.0, 15.0, 13.0),
        (50.0, 12.0, 11.0))
FAN_T = 0.9
# The rock: the volume's voxel and reach, the slab facets (seed spacing across the fall line,
# elongation along it, lift, tilt), the joints (width, depth), the weathering, the tris per LOD.
VOX = 0.25
VX0, VX1, VZ0, VZ1 = -31.0, 31.0, -4.0, 38.0
SLAB_S, SLAB_ALPHA, SLAB_LIFT, SLAB_TILT = 2.8, 0.36, 0.8, 7.0
DIL, CARVE = 1.1, 1.4
JOINT_W, JOINT_D = (0.12, 0.3), 0.6
SMOOTH_ITER = 7
ROCK_TRIS = (7600, 2750, 520)
UV_SMOOTH = 5  # smoothing passes on the copy the rock islands are cut on
BURY = 3.0  # the rock's foot below the ground (m)


def _bearing_field(x, y, sigma_deg):
    """Per-spine angular weights exp(-(dth / sigma)^2) round the summit, shape (n_spines, ...)."""
    th = np.arctan2(y - SY, x - SX)
    out = []
    for ang, _l, _w0, _w1 in SPINES:
        dth = np.angle(np.exp(1j * (th - math.radians(ang))))
        out.append(np.exp(-(dth / math.radians(sigma_deg)) ** 2))
    return np.array(out)


def slope_h(x, y):
    """The grass slopes: monotonic along every ray from the summit, a grass rib under each spine
    (higher, reaching further) and lower ground under the gullies."""
    x, y = np.asarray(x, float), np.asarray(y, float)
    r = np.hypot(x - SX, y - SY)
    th = np.arctan2(y - SY, x - SX)
    rib = np.max(_bearing_field(x, y, 14.0), axis=0)
    wob = fbm(np.cos(th) * 2.5, np.sin(th) * 2.5, 1.0, 3, 11) - 0.5
    r_out = R_OUT * (1.0 + 0.08 * wob) + 4.0 * rib
    t = np.clip((r_out - r) / (r_out - R_IN), 0.0, 1.0)
    return A_G * (0.82 + 0.3 * rib + 0.2 * wob) * t ** P_G


def mass(x, y):
    """The rock mass's top surface (absolute height): the tops and the spines, smooth-maxed. Where
    it falls under the grass (gullies, flanks, beyond the spine tips) the rock is buried."""
    x, y = np.asarray(x, float), np.asarray(y, float)
    g = slope_h(x, y)
    m = g - 6.0
    # Clefts: along each gully's bearing the tops are cut down toward the grass from about 3 m
    # off the summit, deepening outward, so the buttresses stand apart over scree-filled clefts.
    th = np.arctan2(y - SY, x - SX)
    rs = np.hypot(x - SX, y - SY)
    cleft = np.zeros(np.shape(x))
    for ang, _half, _length in FANS:
        dth = np.angle(np.exp(1j * (th - math.radians(ang))))
        cleft = np.maximum(cleft, np.exp(-(dth / CLEFT_SIGMA) ** 2))
    keep = 1.0 - CLEFT_DEPTH * cleft * sstep(3.0, 10.0, rs)
    for tx, ty, h, rad, p, q, ex in TOPS:
        dx, dy = (x - tx) / ex, y - ty
        d = np.sqrt(dx * dx + dy * dy + 4.0) - 2.0  # a rounded top, not a cone point
        a = np.arctan2(dy, dx)
        d = d * (1.0 + 0.18 * (fbm(np.cos(a) * 2.0 + tx, np.sin(a) * 2.0 + ty, 1.0, 3, 5) - 0.5))
        top = h * np.clip(1.0 - (d / rad) ** q, 0.0, 1.0) ** p - 6.0 * np.maximum(d - rad, 0.0) / rad
        m = smax(m, g + (top - g) * keep, 1.5)
    for ang, length, w0, w1 in SPINES:
        a = math.radians(ang)
        ux, uy = math.cos(a), math.sin(a)
        u = (x - SX) * ux + (y - SY) * uy
        v = -(x - SX) * uy + (y - SY) * ux
        # The crest wanders a little across its bearing.
        v = v - 1.2 * (fbm(u * 0.15 + ang, 0.0 * u, 1.0, 2, 41) - 0.5) * np.clip(u / length, 0.0, 1.0)
        uc = np.clip(u / length, 0.0, 1.0)
        w = w0 + (w1 - w0) * uc
        e = SPINE_E0 * (1.0 - uc) ** 1.25 + 0.8
        crest = (g + e * np.clip(1.0 - (np.abs(v) / w) ** 1.7, -1.0, 1.0) - 3.0 * np.maximum(-u - 1.0, 0.0)
                 - np.maximum(u - length, 0.0))
        m = smax(m, crest, 1.2)
    return m


def soft_outline_dist(x, y):
    """The distance to the outer edges (inside the outline, else 0) with the creases where two
    edges are equally near rounded: a polynomial smooth min of FLANK_SOFT over the edge distances,
    so the flank has no hip ridges off the outline's corners."""
    x, y = np.asarray(x, float), np.asarray(y, float)
    d = None
    for (ax, ay), (bx, by) in SEGS:
        dx, dy = bx - ax, by - ay
        t = np.clip(((x - ax) * dx + (y - ay) * dy) / (dx * dx + dy * dy), 0.0, 1.0)
        e = np.hypot(x - ax - t * dx, y - ay - t * dy)
        if d is None:
            d = e
        else:
            h = np.clip(0.5 + 0.5 * (e - d) / FLANK_SOFT, 0.0, 1.0)
            d = e + (d - e) * h - FLANK_SOFT * h * (1.0 - h)
    return np.where(outline_dist(x, y) > 0.0, np.maximum(d, 0.0), 0.0)


def flank(x, y):
    """The grass flank: 0 on the outline with no slope there, monotonic in the outline distance."""
    d = soft_outline_dist(x, y)
    return FLANK_H * sstep(0.0, FLANK_D, d) + FLANK_H2 * sstep(0.0, FLANK_D2, d)


def grass_h(x, y):
    """The grass before the scree: the form pass's slopes, faded out toward the rim, and the flank,
    smooth-maxed (the max's reach fading out at the rim, where both are 0)."""
    x, y = np.asarray(x, float), np.asarray(y, float)
    d = outline_dist(x, y)
    return smax(np.maximum(slope_h(x, y), 0.0) * sstep(2.5, 7.0, d), flank(x, y), FLANK_K * sstep(0.0, 8.0, d))


def _fan_apexes():
    """Each fan's apex: the rock foot along its ray from the summit (where the mass first drops
    under the grass plus 0.5 m)."""
    out = []
    for ang, half, length in FANS:
        a = math.radians(ang)
        r0 = 3.0
        while r0 < 24.0:
            p = (np.array([SX + r0 * math.cos(a)]), np.array([SY + r0 * math.sin(a)]))
            if mass(*p)[0] < grass_h(*p)[0] + 0.5:
                break
            r0 += 0.25
        out.append((a, SX + (r0 - 1.0) * math.cos(a), SY + (r0 - 1.0) * math.sin(a), math.radians(half), length))
    return out


FAN_APEX = _fan_apexes()


def fan_lift(x, y):
    """The scree lens over the slope: FAN_T thick at its axis near the apex, thinning to 0 at the
    wedge sides and the toe, so a fan never ends in a step."""
    lift = np.zeros(np.shape(x))
    th = np.arctan2(y - SY, x - SX)
    rs = np.hypot(x - SX, y - SY)
    grain = 0.8 + 0.4 * fbm(np.cos(th) * 9.0, np.sin(th) * 9.0, 1.0, 2, 97)
    for a, ax, ay, half, length in FAN_APEX:
        r0 = math.hypot(ax - SX, ay - SY)
        off = np.abs(np.angle(np.exp(1j * (th - a))))
        side = 1.0 - sstep(0.45 * half, half, off)
        # Thickest at the apex (under the rock edge), thinning outward: monotone along the ray.
        along = sstep(r0 - 1.5, r0, rs) * (1.0 - sstep(r0 + 0.35 * length, r0 + length, rs))
        lift = np.maximum(lift, FAN_T * side * along * grain)
    return lift


def rise(x, y):
    """Ground height above the meadow plane (grass and scree, no rock)."""
    return grass_h(x, y) + fan_lift(x, y) * sstep(2.5, 7.0, outline_dist(x, y))


def meadow_depth(x, y):
    """meadow_base's hollow depth (`_terrain.meadow_depth`), vectorized."""
    t = np.clip((_terrain.undulation(x, y, np) - _terrain.HOLLOW_FROM) / (1.0 - _terrain.HOLLOW_FROM), 0.0, 1.0)
    return _terrain.MEADOW_DEPTH * t * t


def height(x, y):
    """The ground cap's height: the rise plus meadow's hollows where the ground is flat."""
    x, y = np.asarray(x, float), np.asarray(y, float)
    r = rise(x, y)
    fall = sstep(0.0, _terrain.FALLOFF, outline_dist(x, y))
    return r - fall * meadow_depth(x, y) * (1.0 - sstep(0.0, 0.6, r))


def classify(x, y):
    """Ground slot per face centre: 'scree' on the fans, else 'cap' (grass)."""
    jitter = 1.6 * (fbm(x, y, 2.0, 3, 91) - 0.5)  # lobed fan outlines, not straight edges
    return np.where(fan_lift(x, y) > 0.35 + 0.3 * jitter, "scree", "cap")


# --- the rock: the mass as a volume, slab facets and joints carved into it ------------------------

def _mass_grad(x, y, e=0.05):
    return ((mass(x + e, y) - mass(x - e, y)) / (2 * e), (mass(x, y + e) - mass(x, y - e)) / (2 * e))


def slab_seeds():
    """Seeds of the slab facets on the exposed mass: (point, plane normal, fall direction, lift,
    joint width). Greedy thinning under a metric stretched along the fall line, so a slab is about
    1 / SLAB_ALPHA times longer down the slope than across it."""
    rng = np.random.default_rng(20261029)
    s = 1.1
    g = np.arange(VX0 + 1.0, VX1 - 1.0, s)
    X, Y = np.meshgrid(g, g, indexing="ij")
    x = (X + rng.uniform(-0.4, 0.4, X.shape) * s).ravel()
    y = (Y + rng.uniform(-0.4, 0.4, Y.shape) * s).ravel()
    m, gr = mass(x, y), slope_h(x, y)
    keep = m > gr + 0.3
    x, y, m = x[keep], y[keep], m[keep]
    gx, gy = _mass_grad(x, y)
    gn = np.hypot(gx, gy)
    rx, ry = x - SX, y - SY
    rn = np.maximum(np.hypot(rx, ry), 1e-6)
    hx, hy = np.where(gn > 0.05, -gx / np.maximum(gn, 1e-6), rx / rn), np.where(gn > 0.05, -gy / np.maximum(gn, 1e-6), ry / rn)
    fall = np.column_stack([hx, hy, -gn])
    fall /= np.linalg.norm(fall, axis=1, keepdims=True)
    nrm = np.column_stack([-gx, -gy, np.ones_like(gx)])
    nrm /= np.linalg.norm(nrm, axis=1, keepdims=True)
    pts = np.column_stack([x, y, m])
    order = rng.permutation(len(pts))
    acc = []
    for k in order:
        p, f = pts[k], fall[k]
        ok = True
        for j in acc:
            d = p - pts[j]
            a = d @ fall[j]
            if d @ d - (1.0 - SLAB_ALPHA ** 2) * a * a < SLAB_S * SLAB_S:
                ok = False
                break
        if ok:
            acc.append(k)
    acc = np.array(acc)
    n = len(acc)
    # Tilt each facet a little about a random axis in its plane; lift some proud of the mass.
    nr = nrm[acc].copy()
    ax = np.cross(nr, rng.normal(size=(n, 3)))
    ax /= np.linalg.norm(ax, axis=1, keepdims=True)
    ang = np.radians(SLAB_TILT) * rng.uniform(0.0, 1.0, n)
    nr = nr * np.cos(ang)[:, None] + np.cross(ax, nr) * np.sin(ang)[:, None]
    lift = SLAB_LIFT * rng.uniform(0.0, 1.0, n) ** 1.5
    jw = rng.uniform(*JOINT_W, n)
    return pts[acc], nr, fall[acc], lift, jw


def rock_volume():
    """The rock's signed distance on a VOX grid (negative inside): the mass between its top and
    BURY under the grass, intersected with each slab facet's plane, joints cut along the facet
    borders; returns (array, origin)."""
    xs = np.arange(VX0, VX1 + 1e-9, VOX)
    zs = np.arange(VZ0, VZ1 + 1e-9, VOX)
    X, Y = np.meshgrid(xs, xs, indexing="ij")
    M = mass(X, Y)
    Gr = height(X, Y)
    gx, gy = np.gradient(M, VOX)
    norm = np.sqrt(1.0 + gx * gx + gy * gy)
    D = ((zs[None, None, :] - M[:, :, None]) / norm[:, :, None]).astype(np.float32)
    D = np.maximum(D, ((Gr - BURY)[:, :, None] - zs[None, None, :]).astype(np.float32))
    q, nr, fall, lift, jw = slab_seeds()
    band = np.abs(D) < 2.2
    ii, jj, kk = np.nonzero(band)
    P = np.column_stack([xs[ii], xs[jj], zs[kk]])
    dh = D[ii, jj, kk].astype(np.float64)
    # Each voxel takes the slab cell of its foot on the mass surface (along the heightfield normal),
    # so a cell is a column through the outer layer and its facet does not change with depth.
    nh = np.stack([-gx, -gy, np.ones_like(gx)], axis=-1) / norm[:, :, None]
    PS = P - dh[:, None] * nh[ii, jj]
    out = np.empty(len(P))
    a2 = 1.0 - SLAB_ALPHA ** 2
    for s0 in range(0, len(P), 40000):
        p = P[s0:s0 + 40000]
        d = PS[s0:s0 + 40000][:, None, :] - q[None, :, :]
        al = np.einsum("pnk,nk->pn", d, fall)
        m2 = np.einsum("pnk,pnk->pn", d, d) - a2 * al * al
        two = np.argpartition(m2, 1, axis=1)[:, :2]
        r = np.arange(len(p))
        m2a, m2b = m2[r, two[:, 0]], m2[r, two[:, 1]]
        swap = m2b < m2a
        i1 = np.where(swap, two[:, 1], two[:, 0])
        d1, d2 = np.sqrt(np.minimum(m2a, m2b)), np.sqrt(np.maximum(m2a, m2b))
        facet = np.einsum("pk,pk->p", nr[i1], p - q[i1]) - lift[i1]
        dd = dh[s0:s0 + 40000]
        v = smax(dd - DIL, facet, 0.7)
        v = np.minimum(v, dd + CARVE)
        border = 0.5 * (d2 - d1)
        groove = np.maximum(border - jw[i1], -(v + JOINT_D))  # JOINT_D under the faceted surface
        v = np.maximum(v, -groove)
        out[s0:s0 + 40000] = v
    D[ii, jj, kk] = out.astype(np.float32)
    # No rock where the mass lies more than 1.5 m under the grass: its dilated skin would stay
    # buried, but it costs triangles and atlas area all round the foot.
    D = np.maximum(D, (0.5 * ((Gr - 1.5) - M))[:, :, None].astype(np.float32))
    print(f"{PIECE}: rock volume {D.shape}, band voxels {len(P)}, slab seeds {len(q)}")
    return D, (VX0, VX0, VZ0)


def _vertex_arrays(me):
    n = len(me.vertices)
    co = np.zeros(n * 3)
    me.vertices.foreach_get("co", co)
    nr = np.zeros(n * 3)
    me.vertex_normals.foreach_get("vector", nr)
    return co.reshape(n, 3), nr.reshape(n, 3)


def _umbrella(co, edges):
    s = np.zeros_like(co)
    cnt = np.zeros(len(co))
    np.add.at(s, edges[:, 0], co[edges[:, 1]])
    np.add.at(s, edges[:, 1], co[edges[:, 0]])
    np.add.at(cnt, edges[:, 0], 1)
    np.add.at(cnt, edges[:, 1], 1)
    return s / np.maximum(cnt, 1)[:, None]


def tri_noise(p, scale, seed):
    """Triplanar value noise in [0, 1] (the mean of three axis-plane noises), for vertical faces."""
    x, y, z = (p / scale).T
    return (vnoise(x, y, seed) + vnoise(x + 3.1, z, seed + 1) + vnoise(y - 1.7, z, seed + 2)) / 3.0


_VOLUME = {}


def volume_mesh(name, step=1):
    """The rock volume meshed by OpenVDB at VOX * step (every step-th voxel), as a mesh datablock."""
    import openvdb
    if "D" not in _VOLUME:
        _VOLUME["D"], _VOLUME["origin"] = rock_volume()
    D, (ox, oy, oz) = _VOLUME["D"], _VOLUME["origin"]
    D = np.ascontiguousarray(D[::step, ::step, ::step])
    grid = openvdb.FloatGrid(background=float(D.max()))
    grid.copyFromArray(D)
    pts, tris, quads = grid.convertToPolygons(isovalue=0.0, adaptivity=0.0)
    verts = (np.asarray(pts, float) * VOX * step + np.array([ox, oy, oz])).tolist()
    faces = [list(f) for f in np.asarray(quads).tolist()] + [list(f) for f in np.asarray(tris).tolist()]
    me = bpy.data.meshes.new(name)
    me.from_pydata(verts, [], faces)
    return me


def rock_coarse(name, tris, mat):
    """LOD2's rock: the volume meshed at 4 voxels, closed, decimated (no seams or open rim to hold
    the collapse), then its buried faces cut and its rim pushed back under the ground."""
    me = volume_mesh(name, 4)
    me.validate()
    bm = bmesh.new()
    bm.from_mesh(me)
    bmesh.ops.recalc_face_normals(bm, faces=bm.faces)
    bmesh.ops.triangulate(bm, faces=bm.faces)
    bm.to_mesh(me)
    bm.free()
    o = scene.link(bpy.data.objects.new(name, me))
    lod.decimate_to(o, int(tris * 1.12))
    cut_buried(o)
    drop_degenerate(o, loose=True)
    rebury_rim(o)
    o.data.materials.append(mat)
    for p in o.data.polygons:
        p.use_smooth = True
    return o


def rock_high(mat):
    """The rock's high poly: the volume meshed (OpenVDB), smoothed (weathered edges), displaced along
    the normals by triplanar grain noise. Point attribute `joint` (0-1) marks the clefts (the
    surface's concavity), for the moss and dark joints."""
    me = volume_mesh(f"{PIECE}_rock_high")
    me.validate()
    bm = bmesh.new()
    bm.from_mesh(me)
    bmesh.ops.recalc_face_normals(bm, faces=bm.faces)
    bmesh.ops.triangulate(bm, faces=bm.faces)
    bm.to_mesh(me)
    bm.free()
    obj = scene.link(bpy.data.objects.new(f"{PIECE}_rock_high", me))
    scene.select_only([obj])
    m = obj.modifiers.new("smooth", "SMOOTH")
    m.factor, m.iterations = 0.5, SMOOTH_ITER
    bpy.ops.object.modifier_apply(modifier=m.name)
    me = obj.data
    me.update()
    co, nr = _vertex_arrays(me)
    disp = (0.35 * (tri_noise(co, 3.0, 3) - 0.5) + 0.22 * (tri_noise(co, 1.1, 5) - 0.5)
            + 0.1 * (tri_noise(co, 0.45, 9) - 0.5))
    co = co + nr * disp[:, None]
    me.vertices.foreach_set("co", co.ravel())
    me.update()
    co, nr = _vertex_arrays(me)
    e = np.zeros(len(me.edges) * 2, np.int64)
    me.edges.foreach_get("vertices", e)
    e = e.reshape(-1, 2)
    s1 = _umbrella(co, e)
    s2 = _umbrella(_umbrella(s1, e), e)
    joint = sstep(0.03, 0.16, np.einsum("ij,ij->i", s2 - co, nr)).astype(np.float32)
    for name, val in (("rockness", np.ones(len(co), np.float32)), ("scree", np.zeros(len(co), np.float32)),
                      ("joint", joint)):
        a = me.attributes.new(name, "FLOAT", "POINT")
        a.data.foreach_set("value", val)
    for p in me.polygons:
        p.use_smooth = True
    me.materials.append(mat)
    print(f"{PIECE}: rock high poly {len(me.vertices)} verts, {len(me.polygons)} tris")
    return obj


def cut_buried(obj, margin=0.3):
    """Delete the faces whose every vertex is more than `margin` under the ground cap."""
    bm = bmesh.new()
    bm.from_mesh(obj.data)
    bm.verts.ensure_lookup_table()
    co = np.array([v.co[:] for v in bm.verts])
    under = co[:, 2] < height(co[:, 0], co[:, 1]) - margin
    dead = [f for f in bm.faces if all(under[v.index] for v in f.verts)]
    bmesh.ops.delete(bm, geom=dead, context="FACES")
    bm.to_mesh(obj.data)
    bm.free()
    obj.data.update()


def rock_lod(src, name, tris, mat=None, loose=False):
    """A decimated copy of `src` (UVs kept), smooth shaded; `loose` drops its loose bits."""
    o = src.copy()
    o.data = src.data.copy()
    o.name = o.data.name = name
    scene.link(o)
    lod.decimate_to(o, tris)
    drop_degenerate(o, loose=loose)
    rebury_rim(o)
    if mat is not None:
        o.data.materials.clear()
        o.data.materials.append(mat)
    for p in o.data.polygons:
        p.use_smooth = True
    return o


def rebury_rim(obj, keep=0.3, depth=0.4):
    """Push the open bottom rim back under the ground: a decimation can pull a rim vertex up out of
    the ground (LOD1 by 0.65 m), which opens a slit at the rock's foot. Rim vertices less than
    `keep` under the cap go to `depth` under it."""
    bm = bmesh.new()
    bm.from_mesh(obj.data)
    rim = {v for e in bm.edges if e.is_boundary for v in e.verts}
    for v in rim:
        g = float(height(v.co.x, v.co.y))
        if v.co.z > g - keep:
            v.co.z = g - depth
    bm.to_mesh(obj.data)
    bm.free()
    obj.data.update()


def drop_degenerate(obj, loose=False):
    """Remove what would carry a zero tangent, which export_glb refuses: faces with no area in space
    or in UV, and vertices where MikkTSpace gives a zero tangent. Each is collapsed into a
    neighbour along its shortest edge (UVs kept), so the shell stays closed; deleting the faces
    instead left about 65 holes, seen as dark notches up to 17 m above the grass. Faces still bad
    after the collapses are cut, and the count is printed. `loose` also drops loose bits
    (`drop_loose`): LOD1 and LOD2 only. LOD0 as approved keeps its loose faces, since dropping
    them moves its islands. The fourth-decimal tangent flips once blamed on that drop came from
    the threaded tangents that `export_glb`'s single-thread path removes."""
    n0 = len(obj.data.polygons)
    me = obj.data

    def collapse(pick):
        bm = bmesh.new()
        bm.from_mesh(me)
        bm.verts.ensure_lookup_table()
        edges = {min(es, key=lambda e: e.calc_length()) for es in pick(bm) if es}
        if edges:
            bmesh.ops.collapse(bm, edges=list(edges), uvs=True)
        bm.to_mesh(me)
        bm.free()
        me.update()
        return len(edges)

    def flat_faces(bm):
        uvl = bm.loops.layers.uv.get("UVMap")
        out = []
        for f in bm.faces:
            uv_area = 1.0
            if uvl is not None and len(f.loops) == 3:
                a, b, c = (lp[uvl].uv for lp in f.loops)
                uv_area = abs((b - a).cross(c - a)) / 2.0
            if f.calc_area() < 1e-6 or uv_area < 1e-11:
                out.append(list(f.edges))
        return out

    def bad_verts():
        if not me.uv_layers:
            return set()
        me.calc_tangents(uvmap="UVMap")
        bad = {lp.vertex_index for lp in me.loops if not lp.tangent.length >= 0.5}  # NaN too
        me.free_tangents()
        return bad

    for _ in range(4):
        if not collapse(flat_faces):
            break
    for _ in range(6):
        bad = bad_verts()
        if not bad:
            break
        collapse(lambda bm: [list(bm.verts[i].link_edges) for i in bad])
    bad = bad_verts()
    if bad:
        bm = bmesh.new()
        bm.from_mesh(me)
        dead = [f for f in bm.faces if any(v.index in bad for v in f.verts)]
        bmesh.ops.delete(bm, geom=dead, context="FACES")
        bm.to_mesh(me)
        bm.free()
        me.update()
    loose = drop_loose(obj) if loose else 0
    filled = fill_holes(obj)
    if filled:
        for _ in range(2):
            bad = bad_verts()
            if not bad:
                break
            collapse(lambda bm: [list(bm.verts[i].link_edges) for i in bad])
    print(f"{PIECE}: drop_degenerate on {obj.name}: {n0 - len(me.polygons)} fewer faces, {len(bad)} vertices cut, "
          f"{loose} loose faces dropped, {filled} holes filled")


def drop_loose(obj, max_faces=8):
    """Delete the shell's loose bits of at most `max_faces` faces: a decimation left one lone
    triangle floating at the surface 11 m up in LOD1, which reads as a hole (its edges are open).
    Returns the number of faces deleted."""
    me = obj.data
    bm = bmesh.new()
    bm.from_mesh(me)
    bm.faces.ensure_lookup_table()
    seen, dead = set(), []
    for f0 in bm.faces:
        if f0.index in seen:
            continue
        part, stack = [], [f0]
        seen.add(f0.index)
        while stack:
            f = stack.pop()
            part.append(f)
            for e in f.edges:
                for g in e.link_faces:
                    if g.index not in seen:
                        seen.add(g.index)
                        stack.append(g)
        if len(part) <= max_faces:
            dead.extend(part)
    if dead:
        bmesh.ops.delete(bm, geom=dead, context="FACES")
    bm.to_mesh(me)
    bm.free()
    me.update()
    return len(dead)


def fill_holes(obj, above=0.3):
    """Close every open loop of the shell that stands clear of the ground; the buried rim loop stays
    open. A filled face takes, per
    corner, the UV its vertex has in a neighbouring face. Returns the number of faces added."""
    me = obj.data
    bm = bmesh.new()
    bm.from_mesh(me)
    uvl = bm.loops.layers.uv.get("UVMap")
    edges = [e for e in bm.edges if e.is_boundary
             and all(v.co.z > float(height(v.co.x, v.co.y)) + above for v in e.verts)]
    added = []
    if edges:
        added = bmesh.ops.holes_fill(bm, edges=edges, sides=12)["faces"]
        if added:
            added = bmesh.ops.triangulate(bm, faces=added)["faces"]
        new = set(added)
        for f in added:
            nb = next((lf.face for e in f.edges for lf in e.link_loops if lf.face not in new), None)
            if nb is not None:
                f.material_index = nb.material_index
            f.smooth = True
            if uvl is not None:
                for lp in f.loops:
                    src = next((lv for lv in lp.vert.link_loops if lv.face not in new), None)
                    if src is not None:
                        lp[uvl].uv = src[uvl].uv
    bm.to_mesh(me)
    bm.free()
    me.update()
    return len(added)


def bvh_of(obj):
    from mathutils.bvhtree import BVHTree
    me = obj.data
    return BVHTree.FromPolygons([v.co[:] for v in me.vertices], [list(p.vertices) for p in me.polygons])


def covered(bvh, pts):
    """True where a ray down from above hits the rock more than 0.15 m over the point."""
    out = np.zeros(len(pts), bool)
    for k, (x, y, z) in enumerate(pts):
        hit = bvh.ray_cast(Vector((x, y, 80.0)), Vector((0.0, 0.0, -1.0)))
        out[k] = hit[0] is not None and hit[0].z > z + 0.15
    return out


def surface_error(bvh, pts):
    """(max, p99) distance from points to the surface in `bvh` (m)."""
    d = np.array([(Vector(p) - bvh.find_nearest(Vector(p))[0]).length for p in pts])
    return round(float(d.max()), 3), round(float(np.percentile(d, 99)), 3)


# --- the ground cap per LOD ---------------------------------------------------------------------

def grad(x, y, e=0.05):
    return ((height(x + e, y) - height(x - e, y)) / (2 * e), (height(x, y + e) - height(x, y - e)) / (2 * e))


def interior_points(sp):
    """Graded triangular-lattice points: sp = (steep, mid, gentle, flat ring) spacings; the flat
    ring (no rise) takes the last, or none. The classes come from the ground less the grass flank,
    and the flank's own points take the gentle class's spacing (LOD2, with no flat-ring points:
    the mid class's on the rim's ease-in); its chord error is the cap's groundMaxHeightErrorM."""
    s0 = sp[0]
    row = s0 * math.sqrt(3.0) / 2.0
    n = int(SPAN / 2 / row) + 2
    m = int(SPAN / 2 / s0) + 2
    j, i = np.mgrid[-n:n + 1, -m:m + 1]
    x = (i * s0 + np.where(j % 2, s0 / 2, 0.0)).ravel()
    y = (j * row).ravel()
    i, j = i.ravel(), j.ravel()
    d = outline_dist(x, y)
    keep = d >= 0.55
    x, y, i, j, d = x[keep], y[keep], i[keep], j[keep], d[keep]
    r = rise(x, y)
    gx, gy = grad(x, y)
    e = 0.05
    fl = flank(x, y)
    fgx, fgy = (flank(x + e, y) - flank(x - e, y)) / (2 * e), (flank(x, y + e) - flank(x, y - e)) / (2 * e)
    on_flank = fl > 0.05
    # LOD2 keeps the mid class's spacing on the steep flank's outer 6 m, the rim's ease-in, and
    # the gentle class's inside it (LOD2 <= 0.15 LOD0 leaves no room for more).
    steep_flank = on_flank & (np.hypot(fgx, fgy) >= MID) & (sp[3] is None) & (d < 6.0)
    r, gx, gy = r - fl, gx - fgx, gy - fgy
    g = np.hypot(gx, gy)

    def sub(step):
        k = max(1, int(round(step / s0)))
        k += k % 2 if k > 1 else 0
        return (i % k == 0) & (j % k == 0)

    steep = (r > 0.05) & (g >= STEEP)
    flank_only = (r <= 0.05) & on_flank
    mid = ((r > 0.05) & (g >= MID) & (g < STEEP) | flank_only & steep_flank) & sub(sp[1])
    # The flank rides on the gentle class's spacing: its bend (most at the rim's ease-in) would
    # cost about 0.5 m of chord error at the flat ring's.
    gentle = ((r > 0.05) & (g < MID) | flank_only & ~steep_flank) & sub(sp[2])
    flat = (r <= 0.05) & ~on_flank & (sub(sp[3]) if sp[3] else np.zeros_like(r, bool)) & (d >= 0.5 * (sp[3] or 0))
    sel = steep | mid | gentle | flat
    return list(zip(x[sel].tolist(), y[sel].tolist()))


def fan_points(step):
    """Points along each fan's axis and flanks every `step` m, so every LOD carries the fans, and
    along its sides (where the lens thins out), which the flank's coarser lattice would otherwise
    cut across with chords dipping up to 12 cm."""
    pts = []
    for a, ax, ay, half, length in FAN_APEX:
        for t in np.arange(1.5, 0.9 * length, step):
            for o in (-1.0 * half, -0.75 * half, -0.5 * half, 0.0, 0.5 * half, 0.75 * half, 1.0 * half):
                pts.append((ax + t * math.cos(a + o), ay + t * math.sin(a + o)))
    return pts


def spaced(pts, dmin):
    """`pts` in order, less any point within `dmin` of one kept before it (no CDT slivers)."""
    cell, grid, out = dmin, {}, []
    for x, y in pts:
        gx, gy = int(math.floor(x / cell)), int(math.floor(y / cell))
        if any(math.dist((x, y), q) < dmin for i in (-1, 0, 1) for j in (-1, 0, 1)
               for q in grid.get((gx + i, gy + j), ())):
            continue
        grid.setdefault((gx, gy), []).append((x, y))
        out.append((x, y))
    return out


def triangulate(pts):
    """CDT of the flower ring plus interior points: (verts, faces up, border flags)."""
    from mathutils.geometry import delaunay_2d_cdt
    coords = [Vector((x, y)) for x, y in RING] + [Vector((x, y)) for x, y in pts]
    nb = len(RING)
    out_v, _e, out_f, orig_v, _oe, _of = delaunay_2d_cdt(coords, [], [list(range(nb))], 1, 1e-6)
    border = np.array([any(k < nb for k in o) for o in orig_v])
    xy = np.array([(v.x, v.y) for v in out_v])
    z = np.where(border, 0.0, height(xy[:, 0], xy[:, 1]))
    verts = np.column_stack([xy, z])
    faces = []
    for f in out_f:
        a, b, c = (verts[k] for k in f[:3])
        faces.append(list(f) if np.cross(b - a, c - a)[2] > 0 else list(reversed(f)))
    return verts, np.array(faces), border


def cap_mesh(i, rock_bvh):
    """The LOD's ground cap: the graded lattice plus the fan points, less the faces its rock covers."""
    pts = interior_points(SPACING[i])
    fp = fan_points(FAN_STEP[i])
    pts = [q for q in pts if min(math.dist(q, t) for t in fp) > 0.4 * FAN_STEP[i]] + fp
    pts = spaced(pts, 0.4 * SPACING[i][0])
    verts, faces, border = triangulate(pts)
    cov = covered(rock_bvh, verts)
    cen = verts[faces].mean(axis=1)
    hidden = cov[faces].all(axis=1) & covered(rock_bvh, cen)
    return verts, faces[~hidden], border, int(hidden.sum())


def vertex_normals(verts, faces, border):
    """Area-weighted normals on the peak, meadow's shading normals on the flat ring, +Z on the
    border ring."""
    v = verts
    fn = np.cross(v[faces[:, 1]] - v[faces[:, 0]], v[faces[:, 2]] - v[faces[:, 0]])
    vn = np.zeros_like(v)
    for k in range(3):
        np.add.at(vn, faces[:, k], fn)
    vn /= np.maximum(np.linalg.norm(vn, axis=1, keepdims=True), 1e-12)
    x, y = v[:, 0], v[:, 1]
    r = rise(x, y)
    e = 1e-3
    f = sstep(0.0, _terrain.FALLOFF, outline_dist(x, y))
    sx = -f * (meadow_depth(x + e, y) - meadow_depth(x - e, y)) / (2 * e)
    sy = -f * (meadow_depth(x, y + e) - meadow_depth(x, y - e)) / (2 * e)
    mn = np.column_stack([-sx, -sy, np.ones_like(x)])
    mn /= np.linalg.norm(mn, axis=1, keepdims=True)
    vn = np.where((r <= 0.05)[:, None], mn, vn)
    vn[border] = (0.0, 0.0, 1.0)
    return vn


def make_obj(name, verts, faces, normals, mat):
    """Mesh object from a face subset, keeping only the vertices it uses."""
    used = np.unique(faces)
    remap = -np.ones(len(verts), np.int64)
    remap[used] = np.arange(len(used))
    me = bpy.data.meshes.new(name)
    me.from_pydata(verts[used].tolist(), [], remap[faces].tolist())
    me.validate(clean_customdata=False)
    for p in me.polygons:
        p.use_smooth = True
    me.materials.append(mat)
    me.uv_layers.new(name="UVMap")
    me.normals_split_custom_set_from_vertices(normals[used].tolist())
    me.update()
    return scene.link(bpy.data.objects.new(name, me))


def planar_ground_uv(obj):
    """Ground (and skirt-free) faces: planar into the atlas's lower-left block [0, G]^2."""
    me = obj.data
    uvl = me.uv_layers["UVMap"]
    for loop in me.loops:
        co = me.vertices[loop.vertex_index].co
        uvl.data[loop.index].uv = ((co.x / SPAN + 0.5) * GS, (co.y / SPAN + 0.5) * GS)


def unwrap_into(obj, u0, v0, su, sv, prestretch=1.0, seams_from=None):
    """Smart-project obj, pack it into the unit square, then place it at (u0, v0) scaled (su, sv).
    `prestretch` widens the islands before packing so a non-square target keeps them isotropic.
    With `seams_from` (a copy of obj with the same topology), the islands are cut there by smart
    project and obj itself is unwrapped angle-based along those seams, so no UV folds."""
    if seams_from is not None:
        scene.select_only([seams_from])
        bpy.ops.object.mode_set(mode="EDIT")
        bpy.ops.mesh.select_all(action="SELECT")
        bpy.ops.uv.smart_project(angle_limit=math.radians(66.0), island_margin=0.003, area_weight=0.0,
                                 correct_aspect=True, scale_to_bounds=False)
        bpy.ops.uv.select_all(action="SELECT")
        bpy.ops.uv.seams_from_islands(mark_seams=True, mark_sharp=False)
        bpy.ops.object.mode_set(mode="OBJECT")
        seam = [e.use_seam for e in seams_from.data.edges]
        assert len(seam) == len(obj.data.edges)
        for e, sflag in zip(obj.data.edges, seam):
            e.use_seam = sflag
    scene.select_only([obj])
    bpy.ops.object.mode_set(mode="EDIT")
    bpy.ops.mesh.select_all(action="SELECT")
    if seams_from is not None:
        bpy.ops.uv.unwrap(method="ANGLE_BASED", fill_holes=False, correct_aspect=True, margin=0.003)
    else:
        bpy.ops.uv.smart_project(angle_limit=math.radians(66.0), island_margin=0.003, area_weight=0.0,
                                 correct_aspect=True, scale_to_bounds=False)
    # Turn each island to its tightest box first (isotropic space), then widen and pack without
    # turning, so the prestretch stays along u for every island.
    bpy.ops.uv.pack_islands(rotate=True, rotate_method="AXIS_ALIGNED", margin=0.003)
    bpy.ops.object.mode_set(mode="OBJECT")
    uvl = obj.data.uv_layers["UVMap"].data
    for d in uvl:
        d.uv = (d.uv[0] * prestretch, d.uv[1])
    bpy.ops.object.mode_set(mode="EDIT")
    bpy.ops.mesh.select_all(action="SELECT")
    bpy.ops.uv.pack_islands(rotate=False, shape_method="CONCAVE", margin=0.003)
    bpy.ops.object.mode_set(mode="OBJECT")
    uvl = obj.data.uv_layers["UVMap"].data  # the mode switch reallocates the loop data
    us_, vs_ = [d.uv[0] for d in uvl], [d.uv[1] for d in uvl]
    assert min(us_) >= -1e-6 and max(us_) <= 1 + 1e-6 and min(vs_) >= -1e-6 and max(vs_) <= 1 + 1e-6, \
        (obj.name, min(us_), max(us_), min(vs_), max(vs_))
    for d in uvl:
        d.uv = (u0 + d.uv[0] * su, v0 + d.uv[1] * sv)


def texel(obj, px):
    me = obj.data
    uvl = me.uv_layers["UVMap"].data
    vals, area = [], 0.0
    for p in me.polygons:
        uvs = [uvl[li].uv for li in p.loop_indices]
        a = 0.0
        for k in range(1, len(uvs) - 1):
            e1, e2 = uvs[k] - uvs[0], uvs[k + 1] - uvs[0]
            a += abs(e1.x * e2.y - e1.y * e2.x) / 2.0
        if p.area > 1e-9 and a > 1e-12:
            vals.append((a * px * px / p.area) ** 0.5)
            area += p.area
    vals.sort()
    return {"median": round(vals[len(vals) // 2], 2), "min": round(vals[0], 2), "max": round(vals[-1], 2),
            "faces": len(vals), "areaM2": round(area, 1)}


def ground_high_poly(mat):
    """The ground's high poly for the scree bakes: a 0.2 m grid of the cap's height out to
    APRON_R + 1 m, with the shader's `scree` attribute (no rock: `rockness` 0)."""
    s = 0.2
    n = int((APRON_R + 1.0) / s)
    g = np.arange(-n, n + 1) * s
    X, Y = np.meshgrid(g, g)
    Z = height(X.ravel(), Y.ravel()).reshape(X.shape)
    scree = sstep(0.2, 0.5, fan_lift(X.ravel(), Y.ravel())).astype(np.float32)
    ny, nx = X.shape
    verts = np.column_stack([X.ravel(), Y.ravel(), Z.ravel()])
    idx = np.arange(nx * ny).reshape(ny, nx)
    quads = np.column_stack([idx[:-1, :-1].ravel(), idx[:-1, 1:].ravel(), idx[1:, 1:].ravel(), idx[1:, :-1].ravel()])
    keep = np.hypot(X[:-1, :-1], Y[:-1, :-1]).ravel() < APRON_R + 0.8
    me = bpy.data.meshes.new(f"{PIECE}_ground_high")
    me.from_pydata(verts.tolist(), [], quads[keep].tolist())
    me.validate()
    for p in me.polygons:
        p.use_smooth = True
    zero = np.zeros(len(verts), np.float32)
    for name, val in (("rockness", zero), ("scree", scree), ("joint", zero)):
        a = me.attributes.new(name, "FLOAT", "POINT")
        a.data.foreach_set("value", val)
    obj = scene.link(bpy.data.objects.new(f"{PIECE}_ground_high", me))
    obj.data.materials.append(mat)
    return obj


def granite_shader(name):
    """Massif-scale granite after ref-2: pale grey, weathering streaks, dark joints, pale
    green-grey and mustard lichen, dark moss in the joints and on the shaded (north-west) side,
    grey scree. Object coordinates are world metres (the high poly sits at the origin)."""
    m = bpy.data.materials.new(name)
    nt = materials.ensure_nodes(m)
    N, L = nt.nodes, nt.links
    bsdf = next(n for n in N if n.type == "BSDF_PRINCIPLED")
    tc = N.new("ShaderNodeTexCoord")
    geo = N.new("ShaderNodeNewGeometry")

    def attr(nm):
        a = N.new("ShaderNodeAttribute")
        a.attribute_name = nm
        return a.outputs["Fac"]

    def tex(scale, detail=2.0, stretch=None, kind="ShaderNodeTexNoise"):
        t = N.new(kind)
        t.inputs["Scale"].default_value = scale
        if "Detail" in t.inputs:
            t.inputs["Detail"].default_value = detail
        src = tc.outputs["Object"]
        if stretch:
            vm = N.new("ShaderNodeVectorMath")
            vm.operation = "MULTIPLY"
            vm.inputs[1].default_value = stretch
            L.new(src, vm.inputs[0])
            src = vm.outputs[0]
        L.new(src, t.inputs["Vector"])
        return t.outputs["Fac"] if kind == "ShaderNodeTexNoise" else t.outputs["Distance"]

    def rng(src, a, b):
        r = N.new("ShaderNodeMapRange")
        r.inputs["From Min"].default_value, r.inputs["From Max"].default_value = a, b
        r.clamp = True
        L.new(src, r.inputs["Value"])
        return r.outputs["Result"]

    def mul(a, b):
        n = N.new("ShaderNodeMath")
        n.operation = "MULTIPLY"
        for k, x in enumerate((a, b)):
            if isinstance(x, float):
                n.inputs[k].default_value = x
            else:
                L.new(x, n.inputs[k])
        return n.outputs[0]

    def mix(fac, a, b):
        n = N.new("ShaderNodeMix")
        n.data_type = "RGBA"
        ins = {s.identifier: s for s in n.inputs}
        L.new(fac, ins["Factor_Float"])
        for key, x in (("A_Color", a), ("B_Color", b)):
            if isinstance(x, tuple):
                ins[key].default_value = x
            else:
                L.new(x, ins[key])
        return next(s for s in n.outputs if s.identifier == "Result_Color")

    c = materials.srgb
    col = mix(rng(tex(0.12, 3.0), 0.35, 0.68), c("#8e8b83"), c("#cbc7bd"))
    grain = tex(2.2, 2.0)
    col = mix(mul(rng(grain, 0.40, 0.30), 0.55), col, c("#6e6b64"))
    col = mix(mul(rng(grain, 0.62, 0.72), 0.5), col, c("#d8d4ca"))
    col = mix(mul(rng(tex(0.5, 4.0, (1.0, 1.0, 0.12)), 0.50, 0.70), 0.7), col, c("#6d6a63"))
    nrm = N.new("ShaderNodeSeparateXYZ")
    L.new(geo.outputs["Normal"], nrm.inputs[0])
    # Lichen in patches (a broad field gated by a finer one), mustard lichen, black lichen spots:
    # ref-2's rock face measures luma spread 0.24, 21% dark, 6% mustard (rock_colour.py).
    lich = mul(mul(rng(tex(0.12, 3.0), 0.40, 0.58), rng(tex(1.1, 3.0), 0.50, 0.60)), rng(nrm.outputs["Z"], -0.3, 0.3))
    col = mix(mul(lich, 0.85), col, c("#a3b47c"))
    col = mix(mul(mul(rng(tex(0.3, 3.0), 0.56, 0.64), rng(tex(1.6, 2.0), 0.45, 0.55)), 0.95), col, c("#c49b1f"))
    col = mix(mul(rng(tex(3.2, 2.0), 0.64, 0.70), 0.85), col, c("#2c2b28"))
    shade = N.new("ShaderNodeVectorMath")
    shade.operation = "DOT_PRODUCT"
    shade.inputs[1].default_value = (-0.45, 0.89, 0.0)
    L.new(geo.outputs["Normal"], shade.inputs[0])
    moss = N.new("ShaderNodeMath")
    moss.operation = "MAXIMUM"
    L.new(rng(attr("joint"), 0.12, 0.6), moss.inputs[0])
    L.new(mul(rng(shade.outputs["Value"], 0.2, 0.6), rng(tex(0.4, 3.0), 0.46, 0.56)), moss.inputs[1])
    # Moss patches on the lower faces, any aspect (ref-2's front buttresses): below about 12 m.
    oz = N.new("ShaderNodeSeparateXYZ")
    L.new(tc.outputs["Object"], oz.inputs[0])
    low = N.new("ShaderNodeMath")
    low.operation = "MAXIMUM"
    L.new(moss.outputs[0], low.inputs[0])
    L.new(mul(rng(oz.outputs["Z"], 13.0, 7.0), rng(tex(0.35, 3.0), 0.54, 0.58)), low.inputs[1])
    col = mix(low.outputs[0], col, c("#2f4419"))
    col = mix(mul(rng(attr("joint"), 0.05, 0.7), 0.85), col, c("#34322e"))
    sc = mix(rng(tex(5.5, 1.0), 0.38, 0.62), c("#7f7c75"), c("#b4b0a7"))
    sc = mix(rng(tex(1.1, 2.0), 0.6, 0.7), sc, c("#5f5c56"))
    col = mix(attr("scree"), col, sc)
    grass = c("#4f7d32")
    col = mix(mul(rng(attr("rockness"), 0.6, 0.0), rng(attr("scree"), 1.0, 0.0)), col, grass)
    L.new(col, bsdf.inputs["Base Color"])
    rough = N.new("ShaderNodeMapRange")
    rough.inputs["To Min"].default_value, rough.inputs["To Max"].default_value = 0.84, 0.92
    L.new(attr("scree"), rough.inputs["Value"])
    L.new(rough.outputs["Result"], bsdf.inputs["Roughness"])
    return m


def zero_tangents(path):
    """{mesh name: [Blender-frame positions]} of the exported vertices whose TANGENT is not unit."""
    import json
    import struct
    data = pathlib.Path(path).read_bytes()
    n = struct.unpack_from("<I", data, 12)[0]
    j = json.loads(data[20:20 + n])
    at = 20 + n + 8

    def read(i, width):
        a = j["accessors"][i]
        bv = j["bufferViews"][a["bufferView"]]
        stride = bv.get("byteStride", 4 * width) // 4
        v = np.frombuffer(data, np.float32, (a["count"] - 1) * stride + width, at + bv.get("byteOffset", 0) + a.get("byteOffset", 0))
        return np.lib.stride_tricks.as_strided(v, (a["count"], width), (stride * 4, 4))

    out = {}
    for m in j.get("meshes", []):
        for prim in m["primitives"]:
            if "TANGENT" not in prim["attributes"]:
                continue
            t = read(prim["attributes"]["TANGENT"], 4)
            off = np.abs(np.linalg.norm(t[:, :3], axis=1) - 1.0) > export.UNIT_TOL
            if off.any():
                p = read(prim["attributes"]["POSITION"], 3)[off]
                out.setdefault(m["name"], []).extend((float(x), float(-z), float(y)) for x, y, z in p)
    return out


def export_clean(root, lods, path):
    """export_glb (single-threaded), collapsing (UVs kept) the shortest edge at any vertex the exporter
    writes with a zero tangent and exporting again: the exporter's tangents can vanish at a fold
    vertex whose own tangents in Blender are fine (one on LOD0's rock, form pass 2)."""
    for _ in range(4):
        try:
            export.export_glb(root, path, threads=1)
            return
        except ValueError as err:
            bad = zero_tangents(path)
            if not bad:
                raise
            print(f"{PIECE}: zero exported tangents {bad}; collapsing ({err})")
        for body in lods:
            pts = bad.get(body.name, [])
            if not pts:
                continue
            bm = bmesh.new()
            bm.from_mesh(body.data)
            bm.verts.ensure_lookup_table()
            co = np.array([v.co[:] for v in bm.verts])
            edges = set()
            for q in pts:
                v = bm.verts[int(np.argmin(np.linalg.norm(co - np.array(q), axis=1)))]
                if v.link_edges:
                    edges.add(min(v.link_edges, key=lambda e: e.calc_length()))
            bmesh.ops.collapse(bm, edges=list(edges), uvs=True)
            bm.to_mesh(body.data)
            bm.free()
            body.data.update()
    export.export_glb(root, path, threads=1)


# --- build --------------------------------------------------------------------------------------
def build(out: pathlib.Path, generator: str) -> None:
    """The whole pipeline into the stage dir `out`; `generator` is the piece generator's path
    (export and bake reports hash it and this helper as generatorDeps)."""
    skip_bake = os.environ.get("HEXW_SKIP_BAKE") == "1"
    scene.reset_scene(PIECE)
    mats = {
        "cap": materials.pbr_material(f"{PIECE}_cap", materials.srgb("#4f7d32"), 0.9),
        "rock": materials.pbr_material(f"{PIECE}_rock", materials.srgb("#a9a69e"), 0.85),
        "scree": materials.pbr_material(f"{PIECE}_scree", materials.srgb("#a19e96"), 0.9),
        "skirt": materials.pbr_material(f"{PIECE}_skirt", materials.srgb("#6b4a2f"), 0.95),
    }
    granite = granite_shader(f"{PIECE}_high_mat")
    scree_unique = SCREE == "unique"

    # The rock: high poly, then LOD0 (decimated, buried faces cut) and its LOD1/LOD2 after the unwrap.
    high = rock_high(granite)
    # Cut the buried feet off a copy of the high poly before decimating it, so LOD0 spends neither
    # triangles nor texels under the ground.
    feet = high.copy()
    feet.data = high.data.copy()
    scene.link(feet)
    cut_buried(feet, margin=0.4)
    rock0 = rock_lod(feet, f"{PIECE}_lod0_rock", ROCK_TRIS[0], mats["rock"])
    bpy.data.objects.remove(feet, do_unlink=True)
    bpy.context.view_layer.update()
    cut_buried(rock0)
    high_bvh = bvh_of(high)

    lods_parts = [{}, {}, {}]  # per LOD: {slot: obj}
    stats = {}

    def ground(i, rock_obj):
        verts, faces, border, hidden = cap_mesh(i, bvh_of(rock_obj))
        nrm = vertex_normals(verts, faces, border)
        cen = verts[faces].mean(axis=1)
        cls = classify(cen[:, 0], cen[:, 1])
        err = np.abs(cen[:, 2] - height(cen[:, 0], cen[:, 1]))
        stats[f"lod{i}"] = {"capTris": int(len(faces)), "capTrisUnderRock": hidden,
                            "groundMaxHeightErrorM": round(float(err.max()), 3),
                            "groundMaxErrorAt": [round(float(c), 2) for c in cen[int(np.argmax(err))]],
                            "trisPerSlot": {s: int((cls == s).sum()) for s in ("cap", "scree")}}
        for s in ("cap", "scree"):
            sel = faces[cls == s]
            assert len(sel), (i, s)
            lods_parts[i][s] = make_obj(f"{PIECE}_lod{i}_{s}", verts, sel, nrm, mats[s])
            if s == "cap" or not scree_unique:
                planar_ground_uv(lods_parts[i][s])

    ground(0, rock0)
    lods_parts[0]["rock"] = rock0
    def unwrap_together(us, *place):
        """One island set over `us`, placed by `unwrap_into`, split back per object. The islands
        are cut on a smoothed copy (same topology), since the slab steps and grain would otherwise
        split the rock into thousands of small islands (19% of the block filled), and the real
        surface is unwrapped angle-based along those seams (taking the smoothed copy's UVs folded
        triangles, whose zero tangents cost holes)."""
        joined = []
        for smooth in (False, True):
            tmp = [o.copy() for o in us]
            for o, src in zip(tmp, us):
                o.data = src.data.copy()
                scene.link(o)
                if smooth and src.name.endswith("_rock"):
                    scene.select_only([o])
                    m = o.modifiers.new("smooth", "SMOOTH")
                    m.factor, m.iterations = 0.5, UV_SMOOTH
                    bpy.ops.object.modifier_apply(modifier=m.name)
            joined.append(geom.join(tmp, f"{PIECE}_unique_tmp{int(smooth)}"))
        unwrap_into(joined[0], *place, seams_from=joined[1])
        bpy.data.objects.remove(joined.pop(), do_unlink=True)
        joined = joined[0]
        uvj = joined.data.uv_layers["UVMap"].data
        k = 0
        for src in us:
            if not src.data.uv_layers:
                src.data.uv_layers.new(name="UVMap")
            for d in src.data.uv_layers["UVMap"].data:
                d.uv = uvj[k].uv
                k += 1
        bpy.data.objects.remove(joined, do_unlink=True)
        bpy.context.view_layer.update()

    # LOD0's unique islands (rock, and the scree under the unique route) in the right three quarters.
    unwrap_together([rock0] + ([lods_parts[0]["scree"]] if scree_unique else []), G, 0.0, 1.0 - G, 1.0,
                    1.0 / (1.0 - G))
    drop_degenerate(rock0)  # its tangents exist only now that it has UVs
    print(f"{PIECE}: LOD0 unique islands cover {_boulder._islands(rock0, 512).mean() / (1.0 - G):.3f} of their block")
    # LOD1: the rock decimates LOD0 and shares its UVs and texels (0.13% of its area leaves 0.5-2x
    # LOD0's density). LOD2's decimation stretches 4% of its faces across LOD0's islands, so LOD2's
    # rock gets its own islands, with its scree, in a left-column block. The ground is regenerated.
    for i, v0 in ((1, G), (2, 2 * G)):
        if i == 1:
            rk = rock_lod(rock0, f"{PIECE}_lod{i}_rock", ROCK_TRIS[i], loose=True)
        else:
            # LOD2 has its own islands, so it comes from the volume meshed coarse: decimating LOD0
            # (its seams and open rim) stalled at about 830 tris with a 7-9 m error.
            rk = rock_coarse(f"{PIECE}_lod{i}_rock", ROCK_TRIS[i], mats["rock"])
        ground(i, rk)
        lods_parts[i]["rock"] = rk
        own = ([rk] if i == 2 else []) + ([lods_parts[i]["scree"]] if scree_unique else [])
        if own:
            unwrap_together(own, 0.0, v0, GS, GS, 1.0)
            if i == 2:
                drop_degenerate(rk)
    rock_err = {}
    for i in range(3):
        rk = lods_parts[i]["rock"]
        pts = [v.co[:] for v in rk.data.vertices]
        back = [v.co[:] for v in high.data.vertices][::7]
        back = [p for p in back if p[2] > float(height(p[0], p[1])) + 0.3]
        rock_err[f"lod{i}"] = {"rockTris": geom.tri_count(rk), "toHighM": surface_error(high_bvh, pts),
                               "highToLodM": surface_error(bvh_of(rk), back)}
    unique_slots = ("rock", "scree") if scree_unique else ("rock",)
    texels = {f"lod{i}_{s}": texel(lods_parts[i][s], ATLAS) for i in range(3) for s in unique_slots}
    masks_density = {f"lod0_{s}": texel(lods_parts[0][s], MASK_SIZE) for s in ("cap", "scree")
                     if s not in unique_slots}
    top = max(v.co.z for v in rock0.data.vertices)
    rock_reach = max(math.hypot(v.co.x, v.co.y) for v in rock0.data.vertices)
    print(f"{PIECE}: scree route {SCREE}; ground {stats}; rock {rock_err}")
    print(f"{PIECE}: unique texel density (px/m at {ATLAS}) {texels}")
    print(f"{PIECE}: masks density of the masks-only slots (px/m at {MASK_SIZE}) {masks_density}")

    # Skirt (one row, the soil set like meadow_base; UV parked in the spare block of the left column).
    skirts = []
    for i in range(3):
        sk = geom.skirt(f"{PIECE}_lod{i}_skirt", RING, hexgeo.LEVEL_STEP, 1)
        sk.data.uv_layers.new(name="UVMap")
        for d in sk.data.uv_layers["UVMap"].data:
            d.uv = (0.5 * G, 3.5 * G)
        sk.data.materials.append(mats["skirt"])
        skirts.append(sk)

    # --- bakes ----------------------------------------------------------------------------------
    n_px = ATLAS * ATLAS
    albedo = np.zeros((n_px, 4), np.float32)
    albedo[:] = (*materials.srgb("#a9a69e")[:3], 1.0)
    normal = np.zeros((n_px, 4), np.float32)
    normal[:] = (0.5, 0.5, 1.0, 1.0)
    orm = np.zeros((n_px, 4), np.float32)
    orm[:] = (1.0, 0.85, 0.0, 1.0)
    ao_px = np.ones(n_px, np.float32)
    bake_stats = {}
    unique0 = [lods_parts[0][s] for s in unique_slots]
    scree_lods = [lods_parts[i]["scree"] for i in (1, 2)] if scree_unique else []
    rock2 = lods_parts[2]["rock"]

    def rasterize(objs, size):
        mask = np.zeros(size * size, bool)
        for o in objs:
            mask |= _boulder._islands(o, size)
        return mask

    if not skip_bake:
        ground_high = ground_high_poly(granite)
        bpy.context.scene.world = bpy.context.scene.world or bpy.data.worlds.new(f"{PIECE}_world")
        bake.setup_cycles(samples=SAMPLES)
        bpy.context.scene.world.light_settings.distance = AO_DIST
        imgs = {k: bake.new_image(f"{PIECE}.{k}", ATLAS, non_color=k != "albedo") for k in ("albedo", "normal", "rough", "ao")}
        first = True
        for o in unique0 + scree_lods + [rock2]:
            src = [high] if o in (rock0, rock2) else [ground_high]
            kw = dict(cage_extrusion=CAGE, max_ray=MAX_RAY, margin=MARGIN) if o is rock0 else \
                dict(cage_extrusion=ROCK2_CAGE[0], max_ray=ROCK2_CAGE[1], margin=MARGIN) if o is rock2 else \
                dict(cage_extrusion=SCREE_CAGE[0], max_ray=SCREE_CAGE[1], margin=MARGIN)
            bake.bake_emit_input(src, o, "Base Color", imgs["albedo"], clear=first, **kw)
            bake.bake_to(o, "NORMAL", imgs["normal"], highs=src, clear=first, **kw)
            bake.bake_to(o, "ROUGHNESS", imgs["rough"], highs=src, clear=first, **kw)
            first = False
        # AO: LOD0's ground, scree and rock onto themselves (the ground block and LOD0's islands),
        # then the rock's islands again from the high poly (its clefts), then the scree's LOD1/LOD2
        # blocks from LOD0's surface.
        copies = []
        for o in [lods_parts[0][s] for s in ("cap", "scree", "rock")]:
            cp = o.copy()
            cp.data = o.data.copy()
            scene.link(cp)
            copies.append(cp)
        cap0 = geom.join(copies, f"{PIECE}_ao_receiver")
        bake.bake_to(cap0, "AO", imgs["ao"], margin=MARGIN, clear=True)
        bake.bake_to(rock0, "AO", imgs["ao"], highs=[high, ground_high], cage_extrusion=CAGE, max_ray=MAX_RAY,
                     margin=MARGIN, clear=False)
        for o in scree_lods:
            bake.bake_to(o, "AO", imgs["ao"], highs=[cap0], cage_extrusion=SCREE_CAGE[0], max_ray=SCREE_CAGE[1],
                         margin=MARGIN, clear=False)
        bake.bake_to(rock2, "AO", imgs["ao"], highs=[high, ground_high], cage_extrusion=ROCK2_CAGE[0],
                     max_ray=ROCK2_CAGE[1], margin=MARGIN, clear=False)
        px = {}
        for k, im in imgs.items():
            p = bake.save_png(im, str(out / f"{PIECE}.{k}.tmp.png"))
            px[k] = bake.load_png(p)[1]
            os.remove(p)
        isl = rasterize(unique0 + scree_lods + [rock2], ATLAS)
        # Keep the bake's gutter too: filtering at an island edge reads it, and the flat fill there
        # drew a dark line along every UV seam.
        gut = isl.reshape(ATLAS, ATLAS).copy()
        for _ in range(MARGIN):
            g2 = gut.copy()
            g2[1:] |= gut[:-1]
            g2[:-1] |= gut[1:]
            g2[:, 1:] |= gut[:, :-1]
            g2[:, :-1] |= gut[:, 1:]
            gut = g2
        gut = gut.ravel()
        albedo[gut] = px["albedo"][gut]
        normal[gut] = px["normal"][gut]
        orm[gut, 1] = px["rough"][gut, 0]
        ao_px[:] = px["ao"][:, 0]
        orm[gut, 0] = ao_px[gut]
        for a in (albedo, normal, orm):
            a[:, 3] = 1.0
        for o in (cap0, ground_high):
            bpy.data.objects.remove(o, do_unlink=True)
        bpy.context.view_layer.update()

        def st(a):
            return {"texels": int(a.size), "min": round(float(a.min()), 4), "mean": round(float(a.mean()), 4),
                    "p01": round(float(np.percentile(a, 1)), 4), "p05": round(float(np.percentile(a, 5)), 4)}

        isl0 = rasterize([rock0], ATLAS)
        # AO at each rock face's UV centroid, area-weighted: the visible rock, not the gutters.
        uvd = rock0.data.uv_layers["UVMap"].data
        face_ao, face_area = [], []
        for poly in rock0.data.polygons:
            u = sum(uvd[li].uv[0] for li in poly.loop_indices) / len(poly.loop_indices)
            v = sum(uvd[li].uv[1] for li in poly.loop_indices) / len(poly.loop_indices)
            face_ao.append(ao_px[min(ATLAS - 1, int(v * ATLAS)) * ATLAS + min(ATLAS - 1, int(u * ATLAS))])
            face_area.append(poly.area)
        face_ao, face_area = np.array(face_ao), np.array(face_area)
        groundm = rasterize([lods_parts[0]["cap"]], ATLAS)
        bake_stats = {"rockAO": st(ao_px[isl0]), "groundAO": st(ao_px[groundm]),
                      "rockFaceAO": {"areaMean": round(float((face_ao * face_area).sum() / face_area.sum()), 4),
                                     "areaUnder0.1": round(float(face_area[face_ao < 0.1].sum() / face_area.sum()), 4)},
                      "rockAlbedoLuma": st(albedo[isl0, :3].mean(1)), "rockRoughness": st(orm[isl0, 1]),
                      "normalMissFraction": round(float((normal[isl0, 2] < 0.05).mean()), 5)}
        print(f"{PIECE}: bake {bake_stats}")
    bpy.data.objects.remove(high, do_unlink=True)
    bpy.context.view_layer.update()

    # Masks (1024): R snow, G wetness, B night 0, A ground AO (1.0 on the unique islands).
    f = ATLAS // MASK_SIZE
    cc = (np.arange(MASK_SIZE) + 0.5) / MASK_SIZE
    U, V = np.meshgrid(cc, cc)
    gx, gy = (U / GS - 0.5) * SPAN, (V / GS - 0.5) * SPAN
    in_ground = ((U < G) & (V < G)).ravel()
    snow = np.full(MASK_SIZE * MASK_SIZE, 0.55, np.float32)
    wet = np.full(MASK_SIZE * MASK_SIZE, 0.30, np.float32)
    swell = _terrain.undulation(gx, gy, np).ravel()
    snow[in_ground] = 1.0
    wet[in_ground] = (0.7 - 0.2 * swell[in_ground]).astype(np.float32)
    ao_small = ao_px.reshape(MASK_SIZE, f, MASK_SIZE, f).mean(axis=(1, 3)).ravel()
    alpha = np.where(in_ground, ao_small, 1.0).astype(np.float32)
    if scree_unique:
        scr = rasterize([lods_parts[i]["scree"] for i in range(3)], MASK_SIZE)
        snow[scr], wet[scr] = 0.8, 0.45
    masks = np.column_stack([snow, wet, np.zeros_like(snow), alpha]).astype(np.float32)
    bake.write_png(MASK_SIZE, masks, str(out / f"{PIECE}.masks.png"))
    bake.write_png(ATLAS, albedo, str(out / f"{PIECE}.albedo.png"), non_color=False)
    nrm_out = normal if NORMAL_SIZE == ATLAS else _rock._downsample(normal, ATLAS, NORMAL_SIZE, renormal=True)
    bake.write_png(NORMAL_SIZE, nrm_out, str(out / f"{PIECE}.normal.png"))
    bake.write_png(ORM_SIZE, _rock._downsample(orm, ATLAS, ORM_SIZE), str(out / f"{PIECE}.orm.png"))

    # --- join, collider, sockets, export ----------------------------------------------------------
    root = scene.empty(PIECE)
    lods = []
    for i in range(3):
        objs = [lods_parts[i][s] for s in ("cap", "scree", "rock")] + [skirts[i]]
        body = geom.join(objs, f"{PIECE}_lod{i}")
        lod.triangulate(body)
        body.parent = root
        lods.append(body)
    tris = [geom.tri_count(o) for o in lods]
    fixed2 = 2 * len(RING) + len(RING) - 2
    print(f"{PIECE}: tris per LOD {tris}; LOD2 fixed ring + skirt {fixed2}, floor LOD0 >= "
          f"{budgets.min_lod0(fixed2, KIND)}; top {top:.2f} m, rock reach {rock_reach:.2f} m")
    budgets.check(tris, KIND, PIECE)

    col = collider.collider_from(lods[2], "cap", f"{PIECE}_collider")
    col.parent = root
    # Six prop anchors, one on the outer band of each ring cell (meadow_base's scatter-rules hook),
    # on the ground: the grass flank stands about 5 m up there.
    for n in range(6):
        cx, cy = CENTRES[1 + n]
        a = math.atan2(cy, cx)
        x, y = cx + 6.0 * math.cos(a), cy + 6.0 * math.sin(a)
        assert float(outline_dist(x, y)) >= 2.5, (n, x, y)
        export.add_socket(f"anchor-prop-{n}", (x, y, float(height(x, y))), root, kind="prop", radius=2.0)

    export_clean(root, lods, str(out / f"{PIECE}.glb"))
    export.write_export_report(str(out / "export-report.json"), root, PIECE, generator, budget_class="hero")
    bake.write_bake_report(str(out / "bake-report.json"), receiverSignature=bake.receiver_signature([lods[0]]),
                           device=os.environ.get("HEXW_BAKE_DEVICE", "CPU"), samples=SAMPLES, marginPx=MARGIN,
                           cageExtrusionM=CAGE, maxRayDistanceM=MAX_RAY, aoDistanceM=AO_DIST,
                           channels={"albedo": {"size": ATLAS, "colorspace": "srgb"},
                                     "normal": {"size": NORMAL_SIZE, "colorspace": "linear"},
                                     "orm": {"size": ORM_SIZE, "colorspace": "linear"},
                                     "masks": {"size": MASK_SIZE, "colorspace": "linear"}},
                           screeRoute=SCREE, texelDensityPxPerM=texels, masksDensityPxPerM=masks_density,
                           groundStats=stats, rockStats=rock_err, bakeStats=bake_stats, topM=round(top, 3),
                           rockReachM=round(rock_reach, 3), lod2Drops={}, skipped=skip_bake)
