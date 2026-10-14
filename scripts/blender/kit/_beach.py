"""Coast family, beach tiles (owner O1): the bed of the straight beach, the headland and the bay.

Water bed contract (docs/kit/water-contract.md §3, §5, §6). Every height is one function of s, the
signed distance from the tile's coastline, positive seaward, in runtime metres (Blender (x, y) is
runtime (x, -y), so formulas convert first):
- `beach` (straight): s = p . (cos 30, sin 30)
- `beach_outer` (headland): s = |p - C| - 24, C = (-2 apothems, 0), land inside
- `beach_inner` (bay): s = 24 - |p - C|, C = (+2 apothems, 0), sea inside
Profile `height(s)`: 0 for s <= -8, the beach curve to the waterline (-1.4) at s = 0, a shelf to
-1.76 at s = 3, a smoothstep drop to the ocean bed (-4.0) at s = 8. It mirrors
`crates/hex-render/src/water/tiles.rs` `coast_height`, which the kit gate checks every LOD against.

Construction (one continuous cap per LOD, plus the soil skirt):
- Every iso-s line with |s| < 8 crosses the border only on the two beach edges, at the point s m
  from the edge midpoint toward the sea corner, on all three tiles. So the bed is a strip of rows at
  fixed s (ROWS_S) between the beach edges: straight lines on `beach`, arcs about C on the corner
  tiles (the 60 degree sector between the two beach-edge lines, which both pass through C). The
  land (s <= -8) and the sea (s >= 8) beyond the strip are flat.
- Border ring: `hexgeo.border_ring(16)` plus the extra beach-edge samples at EXTRA_S. The ring is a
  function of s only on beach edges, so it is the same on all three tiles and every legal beach
  seam meets vertex for vertex; grass edges sit at 0 and ocean edges at -4, matching meadow_base and
  ocean_base. The rows end on ring points and are the same in every LOD (the profile tolerance binds
  every LOD); LODs change only the vertex spacing along rows and the flat-region lattice.
- Normals are analytic: n = normalize(-h'(s) grad s, 1). On a beach edge grad s runs along the edge
  on all three tiles, so both sides of a seam shade alike.

Material (no new sampled texture: kit forward is at 16 next to refracting water, ADR 0002): the
sand slots tint the shared `plaster` set (fine, low-contrast grain), the underwater slot tints the
shared `gravel` set exactly as ocean_base's seabed does, grass and skirt reuse the meadow sets. The
wet band is its own slot, a darker tint of the same plaster set, from the waterline up to
y = WET_TOP_Y (0.6 m above the water, s = -1.95); its edges are rows, so it
follows the arc.
"""
import math
import os
import pathlib
import sys

import bpy
import numpy as np
from mathutils import Vector
from mathutils.geometry import delaunay_2d_cdt

from lib import bake, collider, export, geom, hexgeo, materials, scene, uv

WATER = -1.4
FLOOR = -hexgeo.LEVEL_STEP  # ocean bed and tile base
BEACH_TOP_S, SHELF_S, DROP_END_S = -8.0, 3.0, 8.0
SHELF_SLOPE = 0.12
ARC_R = 1.5 * hexgeo.HEX_R  # 24 m
ARC_C = 2.0 * hexgeo.APOTHEM  # 27.713 m from the tile centre
SHALLOW_S = 7.36  # water under 2.5 m deep for s below this (contract §3)
WET_TOP_Y = -0.8  # top of the damp band: 0.6 m above the water, wider than the placeholder shader's live
# 0.32-0.40 m band because the kit has no live run-up line (the swash covers up to y -1.18)
TOL_WET, TOL_DRY = 0.03, 0.10
MASK_SIZE = 1024  # 32 px/m over the 32 m square (D2)
# LOD2 ceiling of the corner tiles (kit_gate, the lane-cut ceiling extended): the ring and the
# one-row skirt fix about 256 tris, and the arc rows on the drop (slope up to 0.67) need about 300
# more for the chord sag to stay inside 0.03 m at LOD2 (about 18% of a 3,200-tri LOD0).
CORNER_LOD2_RATIO = 0.20

# Tile layouts at rotation 0 (data/tiles/beach*.ron): beach edges, then the coastline shape.
LAYOUTS = {
    "straight": {"beach": (2, 5), "grass": (3, 4), "ocean": (0, 1)},
    "outer": {"beach": (2, 4), "grass": (3,), "ocean": (0, 1, 5)},
    "inner": {"beach": (1, 5), "grass": (2, 3, 4), "ocean": (0,)},
}

GRASS, SAND, WET, SEABED, SKIRT = "beach_grass", "beach_sand", "beach_wet", "beach_seabed", "beach_skirt"
SLOTS = (GRASS, SAND, WET, SEABED, SKIRT)


def smoothstep(a, b, x):
    t = min(max((x - a) / (b - a), 0.0), 1.0)
    return t * t * (3.0 - 2.0 * t)


def height(s):
    """Bed y (runtime, = Blender z) at s m seaward of the coastline (tiles.rs coast_height)."""
    if s <= BEACH_TOP_S:
        rel = -WATER
    elif s < 0.0:
        t = s / BEACH_TOP_S
        rel = -WATER * t * (2.0 - t)
    else:
        rel = -SHELF_SLOPE * min(s, SHELF_S) + (FLOOR - WATER + SHELF_SLOPE * SHELF_S) * smoothstep(SHELF_S, DROP_END_S, s)
    return WATER + rel


def slope(s, h=1e-4):
    """dh/ds, central difference (the mean of both sides at the s = 0 kink)."""
    return (height(s + h) - height(s - h)) / (2 * h)


def s_of_height(y):
    """The s on the beach slope (-8 < s < 0) where the bed stands at y."""
    t = 1.0 - math.sqrt(1.0 - (y - WATER) / -WATER)
    return BEACH_TOP_S * t


WET_TOP_S = s_of_height(WET_TOP_Y)  # -1.953
# Beach-edge samples beyond the 1 m lattice (shared by all three tiles): the damp band's top, and
# 0.5 m spacing on the smoothstep drop (linear interpolation at 1 m misses it by 0.054 m there).
EXTRA_S = (round(WET_TOP_S, 6), 3.5, 4.5, 5.5, 6.5, 7.5)
ROWS_S = sorted(set(float(s) for s in range(-8, 9)) | set(EXTRA_S))


def seaward(layout, x, y):
    """s at Blender (x, y)."""
    rx, rz = x, -y
    if layout == "straight":
        return rx * math.cos(math.radians(30)) + rz * math.sin(math.radians(30))
    if layout == "outer":
        return math.hypot(rx + ARC_C, rz) - ARC_R
    return ARC_R - math.hypot(rx - ARC_C, rz)


def grad_s(layout, x, y):
    """Gradient of s in Blender (x, y)."""
    if layout == "straight":
        return (math.cos(math.radians(30)), -math.sin(math.radians(30)))
    cx = -ARC_C if layout == "outer" else ARC_C
    d = math.hypot(x - cx, y) or 1.0
    sign = 1.0 if layout == "outer" else -1.0
    return (sign * (x - cx) / d, sign * y / d)


def normal(layout, x, y):
    g = grad_s(layout, x, y)
    k = -slope(seaward(layout, x, y))
    n = Vector((k * g[0], k * g[1], 1.0))
    return tuple(n.normalized())


def edge_point(layout, k, s):
    """The point on beach edge k at s: s m from the edge midpoint toward the sea corner."""
    a, b = hexgeo.corner((k - 1) % 6), hexgeo.corner(k % 6)
    sea, land = (a, b) if seaward(layout, *a) > seaward(layout, *b) else (b, a)
    m = ((a[0] + b[0]) / 2, (a[1] + b[1]) / 2)
    u = ((sea[0] - land[0]) / hexgeo.HEX_R, (sea[1] - land[1]) / hexgeo.HEX_R)
    return (m[0] + u[0] * s, m[1] + u[1] * s)


def ring(layout):
    """The cap border loop [(x, y, z)]: the 16-per-edge lattice plus EXTRA_S on each beach edge,
    every vertex exactly on the profile. Same in every LOD."""
    lay = LAYOUTS[layout]
    sub = hexgeo.BORDER_SUBDIV
    lattice = hexgeo.border_ring(sub)
    pts = []
    for k in range(6):
        seg = lattice[k * sub:(k + 1) * sub]
        if k in lay["beach"]:
            a = hexgeo.corner((k - 1) % 6)
            extra = [edge_point(layout, k, s) for s in EXTRA_S]
            seg = sorted(seg + extra, key=lambda p: math.hypot(p[0] - a[0], p[1] - a[1]))
        pts += seg
    out = [(x, y, height(seaward(layout, x, y))) for x, y in pts]
    check_ring(layout, out)
    return out


def check_ring(layout, loop):
    """The contract's frame, asserted: s = 0 at both beach-edge midpoints, s >= 8 on ocean edges,
    s <= -8 on grass edges, s on a beach edge = the along-edge distance from the midpoint."""
    lay = LAYOUTS[layout]
    for k in lay["beach"]:
        m = hexgeo.edge_midpoint(k)
        assert abs(seaward(layout, *m)) < 1e-6, (layout, k, seaward(layout, *m))
        for s in (-8.0, -3.0, 5.0, 8.0):
            p = edge_point(layout, k, s)
            assert abs(seaward(layout, *p) - s) < 1e-6, (layout, k, s, seaward(layout, *p))
    for x, y, _z in loop:
        k = edge_of(x, y)
        s = seaward(layout, x, y)
        if k in lay["ocean"] and not corner_of_beach(layout, x, y):
            assert s >= DROP_END_S - 1e-6, (layout, k, x, y, s)
        if k in lay["grass"] and not corner_of_beach(layout, x, y):
            assert s <= BEACH_TOP_S + 1e-6, (layout, k, x, y, s)


def edge_of(x, y):
    """The edge a border point lies on (the first match at a corner)."""
    return max(range(6), key=lambda k: x * hexgeo.edge_normal(k)[0] + y * hexgeo.edge_normal(k)[1])


def corner_of_beach(layout, x, y):
    return any(math.hypot(x - c[0], y - c[1]) < 1e-6
               for k in LAYOUTS[layout]["beach"] for c in (hexgeo.corner((k - 1) % 6), hexgeo.corner(k)))


def interp_error(s):
    """Linear interpolation error between the rows around s: h^2/8 * max|h''| over the gap."""
    i = ROWS_S.index(s)
    err = 0.0
    for a, b in ((ROWS_S[max(i - 1, 0)], s), (s, ROWS_S[min(i + 1, len(ROWS_S) - 1)])):
        if b > a:
            curv = max(abs(slope(a + (b - a) * f + 1e-3) - slope(a + (b - a) * f - 1e-3)) / 2e-3
                       for f in np.linspace(0.02, 0.98, 25))
            err = max(err, (b - a) ** 2 / 8.0 * curv)
    return err


def row_points(layout, s, max_seg, sag_cap):
    """One row at s from beach edge a to beach edge b: straight rows split at most max_seg apart,
    arc rows also short enough that the chord sag times the local slope stays inside what the
    contract tolerance (0.03 m below s = 7.36) leaves after the interpolation error between rows,
    and under `sag_cap`."""
    tol = min(sag_cap, 0.028 - interp_error(s))
    ka, kb = LAYOUTS[layout]["beach"]
    pa, pb = edge_point(layout, ka, s), edge_point(layout, kb, s)
    if layout == "straight":
        n = max(1, math.ceil(math.dist(pa, pb) / max_seg))
        return [(pa[0] + (pb[0] - pa[0]) * i / n, pa[1] + (pb[1] - pa[1]) * i / n) for i in range(n + 1)]
    cx = -ARC_C if layout == "outer" else ARC_C
    r = math.hypot(pa[0] - cx, pa[1])
    a0, a1 = math.atan2(pa[1], pa[0] - cx), math.atan2(pb[1], pb[0] - cx)
    if a1 - a0 > math.pi:
        a1 -= 2 * math.pi
    elif a0 - a1 > math.pi:
        a1 += 2 * math.pi
    g = max(abs(slope(s + d)) for d in (-0.5, -0.25, 0.0, 0.25, 0.5))
    seg = max_seg if g < 1e-9 else min(max_seg, math.sqrt(8.0 * r * tol / g))
    n = max(1, math.ceil(r * abs(a1 - a0) / seg))
    return [(cx + r * math.cos(a0 + (a1 - a0) * i / n), r * math.sin(a0 + (a1 - a0) * i / n)) for i in range(n + 1)]


def slot_of(layout, x, y):
    s = seaward(layout, x, y)
    if s < BEACH_TOP_S:
        return GRASS
    if s < WET_TOP_S:
        return SAND
    if s < 0.0:
        return WET
    return SEABED


def cap(name, layout, max_seg, spacing, sag_cap):
    """The bed surface for one LOD: border loop + the ROWS_S strip (rows constrained) + a flat lattice
    at `spacing` (None: none) beyond the strip, triangulated by constrained Delaunay."""
    loop = ring(layout)
    pts = [(x, y) for x, y, _ in loop]
    zs = [z for _, _, z in loop]
    index = {(round(x, 5), round(y, 5)): i for i, (x, y) in enumerate(pts)}

    def add(x, y, z):
        key = (round(x, 5), round(y, 5))
        if key not in index:
            index[key] = len(pts)
            pts.append((x, y))
            zs.append(z)
        return index[key]

    edges = []
    for s in ROWS_S:
        ids = [add(x, y, height(s)) for x, y in row_points(layout, s, max_seg, sag_cap)]
        edges += list(zip(ids, ids[1:]))
    if spacing:
        for x, y in hexgeo.lattice_points(spacing, spacing * 0.5):
            s = seaward(layout, x, y)
            if s < BEACH_TOP_S - spacing * 0.5 or s > DROP_END_S + spacing * 0.5:
                add(x, y, height(s))
    nb = len(loop)
    out_v, _e, out_f, orig_v, _oe, _of = delaunay_2d_cdt([Vector(p) for p in pts], edges, [list(range(nb))], 1, 1e-6)
    verts = []
    for i, v in enumerate(out_v):
        if not orig_v[i]:
            raise AssertionError(f"{name}: CDT added a Steiner vertex at {tuple(v)}")
        verts.append((v.x, v.y, zs[orig_v[i][0]]))
    faces = []
    for f in out_f:
        a, b, c = (Vector(verts[i]) for i in f[:3])
        faces.append(list(f) if (b - a).cross(c - a).z > 0 else list(reversed(f)))
    mesh = bpy.data.meshes.new(name)
    mesh.from_pydata(verts, [], faces)
    mesh.validate(clean_customdata=False)
    mesh.update()
    obj = scene.link(bpy.data.objects.new(name, mesh))
    for p in obj.data.polygons:
        p.use_smooth = True
        p.material_index = SLOTS.index(slot_of(layout, p.center.x, p.center.y))
    obj.data.normals_split_custom_set_from_vertices([normal(layout, v.co.x, v.co.y) for v in obj.data.vertices])
    obj.data.update()
    return obj


def skirt(name, layout, rows):
    """Soil walls under the border loop down to the tile base, `rows` quads high. Segments already
    at the base (the ocean edges) are skipped; a segment with one end there is one triangle."""
    loop = ring(layout)
    n = len(loop)
    verts, faces = [], []
    for i in range(n):
        a, b = loop[i], loop[(i + 1) % n]
        da, db = a[2] - FLOOR, b[2] - FLOOR
        if da < 1e-6 and db < 1e-6:
            continue
        base = len(verts)
        if da < 1e-6 or db < 1e-6:
            top = b if da < 1e-6 else a
            verts += [(a[0], a[1], FLOOR), (b[0], b[1], FLOOR), top]
            faces.append([base, base + 1, base + 2])
            continue
        for r in range(rows + 1):
            t = r / rows
            verts += [(a[0], a[1], a[2] + (FLOOR - a[2]) * t), (b[0], b[1], b[2] + (FLOOR - b[2]) * t)]
        for r in range(rows):
            p = base + 2 * r
            faces += [[p, p + 2, p + 3], [p, p + 3, p + 1]]
    mesh = bpy.data.meshes.new(name)
    mesh.from_pydata(verts, [], faces)
    mesh.validate(clean_customdata=False)
    mesh.update()
    obj = scene.link(bpy.data.objects.new(name, mesh))
    for p in obj.data.polygons:
        if p.normal.x * p.center.x + p.normal.y * p.center.y < 0:
            p.flip()
    obj.data.update()
    return obj


def ring_key(obj):
    """Cap border vertices (x, y, z) to 0.1 mm."""
    return {(round(v.co.x, 4), round(v.co.y, 4), round(v.co.z, 4)) for v in obj.data.vertices
            if abs(hexgeo.dist_to_border(v.co.x, v.co.y)) < 1e-4}


def profile_errors(obj, layout, samples=4):
    """Max deviation from the profile, on the cap faces of `obj` (skirt walls skipped): at vertices,
    edge midpoints and interior barycentric samples. The task's strict reading of contract §3:
    0.03 m wherever s < SHALLOW_S (7.36), reported as dry (s < 0, above the waterline) and wet
    (0 <= s < 7.36, under water); 0.10 m beyond (deep), upward only on the ocean floor (s >= 8).
    Raises over tolerance."""
    mesh = obj.data
    wet = dry = deep = 0.0
    below = 0.0
    for p in mesh.polygons:
        if p.normal.z < 0.2:
            continue
        cs = [mesh.vertices[i].co for i in p.vertices]
        bary = [(i / samples, j / samples) for i in range(samples + 1) for j in range(samples + 1 - i)]
        for u, v in bary:
            q = cs[0] + (cs[1] - cs[0]) * u + (cs[2] - cs[0]) * v
            s = seaward(layout, q.x, q.y)
            want = height(s)
            err = abs(q.z - want)
            if s >= DROP_END_S:
                below = max(below, want - q.z)
            if s < 0.0:
                dry = max(dry, err)
            elif s < SHALLOW_S:
                wet = max(wet, err)
            else:
                deep = max(deep, err)
    assert wet <= TOL_WET and dry <= TOL_WET and deep <= TOL_DRY and below <= 1e-4, \
        f"{obj.name}: profile error wet {wet:.4f} dry {dry:.4f} deep {deep:.4f} below-bed {below:.4f}"
    return {"wetMaxM": round(wet, 4), "dryMaxM": round(dry, 4), "deepMaxM": round(deep, 4)}


def make_materials():
    return {
        GRASS: materials.pbr_material(GRASS, materials.srgb("#4f7d32"), 0.9),
        SAND: materials.pbr_material(SAND, materials.srgb("#e3d4b4"), 0.9),
        WET: materials.pbr_material(WET, materials.srgb("#9c8565"), 0.5),
        SEABED: materials.pbr_material(SEABED, materials.srgb("#a39579"), 0.85),
        SKIRT: materials.pbr_material(SKIRT, materials.srgb("#6b4a2f"), 0.95),
    }


def masks(layout, out, piece, size=MASK_SIZE):
    """Analytic masks over the planar UV0 (no bake: the bed is a smooth height field and the
    dressing is a separate piece): R snow capacity 1 on land, 0 under water; G wetness 0.7 on dry
    land, 1.0 on the damp band, 0 under water (already wet); B night 0; A AO 1."""
    c = (np.arange(size, dtype=np.float64) + 0.5) / size * 2 * hexgeo.HEX_R - hexgeo.HEX_R
    xs, ys = np.meshgrid(c, c)
    s = np.vectorize(lambda x, y: seaward(layout, x, y))(xs, ys).ravel()
    snow = np.where(s < 0.0, 1.0, 0.0).astype(np.float32)
    wet = np.where(s < WET_TOP_S, 0.7, np.where(s < 0.0, 1.0, 0.0)).astype(np.float32)
    return bake.make_masks(size, snow, wet, 0.0, None, str(out / f"{piece}.masks.png"))


def build_base(piece, layout, generator, max_seg=(0.7, 1.6, 40.0), spacing=(0.75, 1.6, None), skirt_rows=(4, 2, 1),
               sag_cap=(0.012, 0.012, 1.0)):
    """One beach base: three LODs of cap + skirt, ring identical across LODs, every LOD on the
    profile, hull collider, analytic masks, GLB and reports."""
    out = pathlib.Path(sys.argv[sys.argv.index("--") + 1])
    skip_bake = os.environ.get("HEXW_SKIP_BAKE") == "1"
    scene.reset_scene(piece)
    root = scene.empty(piece)
    mats = make_materials()
    lods, rings = [], []
    for i in range(3):
        c = cap(f"{piece}_lod{i}_cap", layout, max_seg[i], spacing[i], sag_cap[i])
        uv.planar_cap_uv(c)
        rings.append(ring_key(c))
        sk = skirt(f"{piece}_lod{i}_skirt", layout, skirt_rows[i])
        sk.data.uv_layers.new(name="UVMap")
        for d in sk.data.uv_layers["UVMap"].data:
            d.uv = (0.005, 0.005)
        for o in (c, sk):
            for name in SLOTS:
                o.data.materials.append(mats[name])
        for p in sk.data.polygons:
            p.material_index = SLOTS.index(SKIRT)
        body = geom.join([c, sk], f"{piece}_lod{i}")
        body.parent = root
        lods.append(body)
    assert all(r == rings[0] for r in rings), f"{piece}: border ring differs across LODs"
    errs = [profile_errors(o, layout) for o in lods]
    tris = [geom.tri_count(o) for o in lods]
    print(f"{piece}: ring {len(rings[0])} vertices, tris per LOD {tris}, profile error per LOD {errs}")
    ratio2 = 0.15 if layout == "straight" else CORNER_LOD2_RATIO
    assert tris[0] <= 4000 and tris[1] <= 0.5 * tris[0] and tris[2] <= ratio2 * tris[0], tris
    col = collider.collider_from(lods[2], "hull", f"{piece}_collider")
    col.parent = root
    texel = uv.measure_texel_density(lods[0], MASK_SIZE)
    if not skip_bake:
        masks(layout, out, piece)
    export.export_glb(root, str(out / f"{piece}.glb"))
    export.write_export_report(str(out / "export-report.json"), root, piece, generator)
    bake.write_bake_report(str(out / "bake-report.json"), receiverSignature=bake.receiver_signature([lods[0]]),
                           device="none", samples=0, marginPx=0,
                           channels={"masks": {"size": MASK_SIZE, "colorspace": "linear"}},
                           reason="analytic masks (snow, wetness by s; AO 1): no bake", texelDensityPxPerM=texel,
                           profileErrorM=errs, rowsS=ROWS_S, extraRingS=list(EXTRA_S), wetTopS=WET_TOP_S,
                           lodTris=tris, lod2MaxRatio=ratio2, skipped=skip_bake)
    return root, lods
