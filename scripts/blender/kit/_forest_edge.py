"""Shared builder for the forest_forest_edge_e<k> family: forest floor over forest_base's grass rim
along edge 0, placed on a tile edge whose own and neighbour sockets are both Forest.

forest_base keeps an untinted grass rim (and a mossy band) so it joins meadow without a joint; two
forest tiles side by side therefore show a grass strip between their litter floors. Each piece is a
litter overlay on edge 0's slice of the tile (|polar angle| <= 30 degrees), one slot of the
world-space `litter` set (as forest_base's floor), so both tiles' halves line up texel for texel.

forest_base's rim contours and relief are not 60-degree symmetric, so the family has one variant per
local edge k of forest_base (owner decision D3): `forest_forest_edge_e<k>` is authored for edge 0 and
placed at local edge k, over the base's wedge k. Each variant is forest_base's own LOD cap for that
wedge, rotated onto edge 0, clipped to the slice and lifted by EPS, so it follows the cap exactly
(nothing floats over the litter) in every LOD:
- Border (x = APOTHEM): the 17 border-ring points at z = EPS in every variant and LOD, mirror-symmetric,
  normals +Z, so the neighbour's piece meets it whatever its own variant is.
- Sides: two variants of one tile meet exactly on the shared corner ray (both are cuts of the same
  base faces). Each runs EXT metres past its rays while diving under the cap, so next to a non-forest
  neighbour the litter ends in a short slope, not a lip (option O-a).
- Inner end: from D_END - DIVE_RUN to D_END the overlay dives under the cap (litter over litter).
- `taper` (option O-b, corner-aware; not shipped): a side flagged True ends on its ray, and the cover
  shrinks toward that ray into forest_base's own rim and mossy band (a `moss` slot), so the litter
  edge follows the base's grass outline at a corner whose next edge is not forest-forest.
The forest_base formulas are a copy of scripts/blender/kit/forest_base.py (approved, frozen); every
build asserts the copy reproduces forest_base LOD0's committed receiver signature."""
import json
import math
import os
import pathlib
import sys

import bmesh
import bpy
import numpy as np
from mathutils import Vector
from mathutils.bvhtree import BVHTree

from lib import bake, collider, export, geom, hexgeo, lod, materials, scene, uv

A = hexgeo.APOTHEM
COS30, SIN30 = math.cos(math.radians(30)), 0.5
TAN30 = SIN30 / COS30
D_END = 6.0  # inner end of the overlay, metres from the border
DIVE_RUN = 0.5  # the inner dive runs over the last DIVE_RUN metres
EXT = 0.6  # side overlap past each corner ray (m, perpendicular to the ray)
DIVE = 0.06  # how far below the cap the overlay ends (m)
EPS = 0.02  # lift over the cap (z-fighting), every LOD
TAPER_DEG = 22.0  # O-b: the cover starts shrinking this many degrees before a tapered ray
MASK_SIZE = 512
FLOOR, MOSS = "forest_forest_edge_floor", "forest_forest_edge_moss"
MOSS_TINT = (0.62, 0.6, 0.42)  # forest_base_edge's manifest colour
REPO = pathlib.Path(__file__).resolve().parents[3]


# --- forest_base copy (scripts/blender/kit/forest_base.py, frozen with its approval) -------------
FB = "forest_base"
FB_SPACING = (0.9, 1.8, None)
FB_CONTOUR_STEP = (0.7, 1.5, 4.0)
FB_SKIRT_ROWS = (4, 2, 1)


def band_rim(phi, m=math):
    return 2.2 + 0.6 * m.sin(5 * phi + 1.3) + 0.35 * m.sin(11 * phi + 0.4) + 0.2 * m.sin(23 * phi + 2.1)


def band_floor(phi, m=math):
    return band_rim(phi, m) + 0.9 + 0.3 * m.sin(7 * phi + 0.7)


def falloff(x, y):
    t = min(max(hexgeo.dist_to_border(x, y) / 2.0, 0.0), 1.0)
    return t * t * (3 - 2 * t)


def undulation(x, y, m=math):
    s = (0.6 * m.sin(0.31 * x + 1.7) * m.cos(0.27 * y - 0.4)
         + 0.4 * m.sin(0.53 * x - 0.45 * y + 0.9))
    return 0.5 + 0.5 * s


def mounds(x, y, m=math):
    s = (0.5 * m.sin(0.9 * x + 0.3) * m.sin(0.8 * y + 1.1)
         + 0.5 * m.sin(1.3 * x - 1.1 * y + 2.0) * m.cos(0.7 * x + 1.4 * y))
    return 0.5 + 0.5 * s


def relief(x, y):
    return -falloff(x, y) * (0.15 * undulation(x, y) + 0.1 * mounds(x, y))


def contour(band, step):
    n = max(12, int(round(6 * 2 * hexgeo.HEX_R * 0.85 / step)))
    pts = []
    for i in range(n):
        phi = 2 * math.pi * (i + 0.5) / n
        ux, uy = math.cos(phi), math.sin(phi)
        reach = max(ux * math.cos(math.radians(-60 * k)) + uy * math.sin(math.radians(-60 * k)) for k in range(6))
        r = (hexgeo.APOTHEM - band(phi)) / reach
        pts.append((r * ux, r * uy))
    return pts


def zone(x, y):
    phi, d = math.atan2(y, x), hexgeo.dist_to_border(x, y)
    return 0 if d < band_rim(phi) else (1 if d < band_floor(phi) else 2)


def forest_base_lod(i):
    """forest_base LOD i (cap + skirt, joined and triangulated), exactly as forest_base.py builds it."""
    spacing, cstep, rows = FB_SPACING[i], FB_CONTOUR_STEP[i], FB_SKIRT_ROWS[i]
    ring = hexgeo.border_ring(hexgeo.BORDER_SUBDIV)
    lines = contour(band_rim, cstep) + contour(band_floor, cstep)
    interior = hexgeo.lattice_points(spacing, spacing * 0.5) if spacing else [(0.0, 0.0)]
    keep = []
    for x, y in interior:
        phi, d = math.atan2(y, x), hexgeo.dist_to_border(x, y)
        if min(abs(d - band_rim(phi)), abs(d - band_floor(phi))) > 0.45 * (spacing or cstep):
            keep.append((x, y))
    cap = geom.cdt_cap(f"{FB}_lod{i}_cap", ring, keep + lines, relief)
    geom.set_border_normals_up(cap)
    uv.planar_cap_uv(cap)
    sk = geom.skirt(f"{FB}_lod{i}_skirt", ring, hexgeo.LEVEL_STEP, rows)
    sk.data.uv_layers.new(name="UVMap")
    for d in sk.data.uv_layers["UVMap"].data:
        d.uv = (0.005, 0.005)
    body = geom.join([cap, sk], f"{FB}_lod{i}")
    lod.triangulate(body)
    return body


# --- the overlay ------------------------------------------------------------------------------
def beyond(x, y, side):
    """Perpendicular distance past the +30 (side 1) or -30 (side -1) corner ray; <= 0 inside."""
    return -SIN30 * x + side * COS30 * y


def base_local(x, y, k):
    """The base-frame point under piece point (x, y) when the piece sits at local edge k."""
    return hexgeo.rotate_xy((x, y), k)


def cover(x, y, k, taper):
    """O-b classification of a piece point: None (leave the base's grass), MOSS or FLOOR."""
    w = 0.0
    for side, on in ((1, taper[0]), (-1, taper[1])):
        if on:
            ang = math.degrees(math.atan2(side * y, x))  # toward this side's ray, 30 at the ray
            t = min(max((ang - (30.0 - TAPER_DEG)) / TAPER_DEG, 0.0), 1.0)
            w = max(w, t * t * (3 - 2 * t))
    if w == 0.0:
        return FLOOR
    bx, by = base_local(x, y, k)
    phi, d = math.atan2(by, bx), hexgeo.dist_to_border(bx, by)
    if d < w * band_rim(phi):
        return None
    return MOSS if d < w * band_floor(phi) else FLOOR


def overlay_lod(name, base_obj, k, taper):
    """forest_base LOD cap for wedge k, rotated onto edge 0, clipped to the slice, lifted by EPS."""
    bm = bmesh.new()
    bm.from_mesh(base_obj.data)
    bm.normal_update()
    bmesh.ops.delete(bm, geom=[f for f in bm.faces if f.normal.z < 0.5], context="FACES")
    for v in bm.verts:
        x, y = base_local(v.co.x, v.co.y, -k)
        if abs(x - A) < 1e-5:  # border ring: snap the rotation's rounding back onto the edge
            x, y = A, float(round(y)) if abs(y - round(y)) < 1e-5 else y
        v.co.x, v.co.y = x, y
    cuts = [((A - D_END, 0.0, 0.0), (1.0, 0.0, 0.0)), ((A - D_END + DIVE_RUN, 0.0, 0.0), (1.0, 0.0, 0.0))]
    for side, on in ((1, taper[0]), (-1, taper[1])):
        n = Vector((-SIN30, side * COS30, 0.0))
        cuts.append(((0.0, 0.0, 0.0), tuple(n)))
        if not on:
            cuts.append((tuple(n * EXT), tuple(n)))
    for co, no in cuts:
        bmesh.ops.bisect_plane(bm, geom=bm.verts[:] + bm.edges[:] + bm.faces[:], dist=1e-6,
                               plane_co=co, plane_no=no)
    ext = {1: 0.0 if taper[0] else EXT, -1: 0.0 if taper[1] else EXT}

    def outside(c):
        return c.x < A - D_END or any(beyond(c.x, c.y, s) > ext[s] + 1e-6 for s in (1, -1))
    bmesh.ops.delete(bm, geom=[f for f in bm.faces if outside(f.calc_center_median())], context="FACES")
    if any(taper):
        # O-b mock: refine the tapered half so the curved cover boundary follows the faces closely.
        def in_taper(c):
            return any(on and math.degrees(math.atan2(s * c.y, c.x)) > 30.0 - TAPER_DEG - 3.0
                       for s, on in ((1, taper[0]), (-1, taper[1])))
        edges = {e for f in bm.faces if in_taper(f.calc_center_median()) for e in f.edges}
        bmesh.ops.subdivide_edges(bm, edges=list(edges), cuts=2, use_grid_fill=True)
        bmesh.ops.triangulate(bm, faces=bm.faces[:])
        bmesh.ops.delete(bm, geom=[f for f in bm.faces if cover(*f.calc_center_median().xy, k, taper) is None],
                         context="FACES")
    bmesh.ops.delete(bm, geom=[v for v in bm.verts if not v.link_faces], context="VERTS")
    for v in bm.verts:
        u = max(min(max(beyond(v.co.x, v.co.y, s) / EXT, 0.0), 1.0) for s in (1, -1))
        t = min(max((A - D_END + DIVE_RUN - v.co.x) / DIVE_RUN, 0.0), 1.0)
        v.co.z += EPS - (EPS + DIVE) * max(u, t)
    bmesh.ops.triangulate(bm, faces=bm.faces[:])
    mesh = bpy.data.meshes.new(name)
    bm.to_mesh(mesh)
    bm.free()
    obj = scene.link(bpy.data.objects.new(name, mesh))
    for p in obj.data.polygons:
        p.use_smooth = True
    return obj


def set_normals(obj):
    mesh = obj.data
    normals = [(0.0, 0.0, 1.0) if abs(v.co.x - A) < 1e-5 else tuple(v.normal) for v in mesh.vertices]
    mesh.normals_split_custom_set_from_vertices(normals)
    mesh.update()


class BaseSampler:
    """Height and zone of one forest_base LOD cap under a piece point placed at local edge k."""

    def __init__(self, obj, k):
        bm = bmesh.new()
        bm.from_mesh(obj.data)
        bm.normal_update()
        self.tree = BVHTree.FromBMesh(bm)
        self.zones = [zone(*f.calc_center_median().xy) if f.normal.z > 0.5 else 3 for f in bm.faces]
        self.k = k
        bm.free()

    def at(self, x, y):
        bx, by = base_local(x * 0.99999, y * 0.99999, self.k)
        hit = self.tree.ray_cast(Vector((bx, by, 5.0)), Vector((0.0, 0.0, -1.0)))
        return None if hit[0] is None else (hit[0].z, self.zones[hit[2]])


def clearance(obj, base):
    """Over the slice core (inside both rays, before the inner dive), on a 0.1 m grid: the minimum
    lift over the cap where it shows rim or mossy band, and the lift over its litter floor."""
    bm = bmesh.new()
    bm.from_mesh(obj.data)
    tree = BVHTree.FromBMesh(bm)
    bm.free()
    worst, floats = 9.0, []
    for x in np.arange(A - D_END + DIVE_RUN + 0.05, A - 0.02, 0.1):
        for y in np.arange(-x * TAN30 + 0.02, x * TAN30 - 0.02, 0.1):
            hit = tree.ray_cast(Vector((x, y, 5.0)), Vector((0.0, 0.0, -1.0)))
            under = base.at(x, y)
            if hit[0] is None or under is None:
                continue
            gap = hit[0].z - under[0]
            if under[1] < 2:
                worst = min(worst, gap)
            else:
                floats.append(gap)
    return {"minClearanceOverRimM": round(worst, 4), "maxFloatOverFloorM": round(max(floats), 4),
            "meanFloatOverFloorM": round(float(np.mean(floats)), 4)}


def build(piece, k, generator, taper=(False, False)):
    """Build variant `piece` for local edge k into the stage dir given after `--`."""
    out = pathlib.Path(sys.argv[sys.argv.index("--") + 1])
    skip_bake = os.environ.get("HEXW_SKIP_BAKE") == "1"
    scene.reset_scene(piece)
    bases = [forest_base_lod(i) for i in range(3)]
    sig = bake.receiver_signature([bases[0]])
    want = json.loads((REPO / "assets-source/kit/forest_base/bake-report.json").read_text())["receiverSignature"]
    assert sig == want, f"forest_base copy drifted: LOD0 signature {sig} != committed {want}"
    root = scene.empty(piece)
    floor = materials.pbr_material(FLOOR, materials.srgb("#5a3f26"), 0.9)
    moss = materials.pbr_material(MOSS, materials.srgb("#3d5a28"), 0.9)
    lods = [overlay_lod(f"{piece}_lod{i}", bases[i], k, taper) for i in range(3)]
    tris = [geom.tri_count(o) for o in lods]
    # LOD ratios: the clipped caps keep forest_base's own LOD ratios, which can sit above 15% for a
    # slice. Splitting LOD0's flat triangles is exact (the surface does not move).
    # Split the longest edges first, so LOD0 grows only as far as the ratios need.
    split_len = 2.0
    while tris[1] > 0.5 * tris[0] or tris[2] > 0.15 * tris[0]:
        split_len *= 0.85
        bm = bmesh.new()
        bm.from_mesh(lods[0].data)
        inner = [e for e in bm.edges if e.calc_length() > split_len and  # never the border ring
                 not all(abs(v.co.x - A) < 1e-5 for v in e.verts)]
        bmesh.ops.subdivide_edges(bm, edges=inner, cuts=1, use_grid_fill=True)
        bmesh.ops.triangulate(bm, faces=bm.faces[:])
        bm.to_mesh(lods[0].data)
        bm.free()
        tris[0] = geom.tri_count(lods[0])
    assert tris[0] <= 4000, tris
    checks = []
    for i, o in enumerate(lods):
        set_normals(o)
        o.data.materials.append(floor)
        if any(taper):
            o.data.materials.append(moss)
            for p in o.data.polygons:
                p.material_index = 1 if cover(*p.center.xy, k, taper) == MOSS else 0
        o.parent = root
        if not any(taper):
            c = clearance(o, BaseSampler(bases[i], k))
            for j in (i - 1, i + 1):  # a VisibilityRange switch can show neighbouring LODs together
                if 0 <= j < 3:
                    c[f"minClearanceOverRimLod{j}M"] = clearance(o, BaseSampler(bases[j], k))["minClearanceOverRimM"]
            checks.append(c)
    for b in bases:
        bpy.data.objects.remove(b)
    bpy.context.view_layer.update()
    print(f"{piece}: tris per LOD {tris}; clearance {checks}")
    assert all(c["minClearanceOverRimM"] >= 0.005 and c["maxFloatOverFloorM"] <= EPS + 0.005 for c in checks), checks
    for o in lods if not any(taper) else []:  # the border section: 17 ring points at EPS, every variant
        sec = sorted((round(v.co.y, 4), round(v.co.z, 4)) for v in o.data.vertices if abs(v.co.x - A) < 1e-4)
        assert sec == [(float(y), EPS) for y in range(-8, 9)], (o.name, sec)

    # Masks UV: planar over the overlay's footprint, U along the edge (y), V inward (x).
    x0 = A - D_END
    y_half = max(abs(v.co.y) for v in lods[0].data.vertices)
    for o in lods:
        layer = o.data.uv_layers.new(name="UVMap")
        for loop in o.data.loops:
            co = o.data.vertices[loop.vertex_index].co
            layer.data[loop.index].uv = ((co.y + y_half) / (2 * y_half), (co.x - x0) / D_END)
    col = collider.collider_from(lods[2], "box", f"{piece}_collider")
    col.parent = root
    export.add_socket("socket-lane-0", (A, 0.0, 0.0), root, kind="forest", edge=0, offset=0.0, baseEdge=k)
    texel = uv.measure_texel_density(lods[0], MASK_SIZE)
    stats = None
    if not skip_bake:
        stats = bake_masks(lods[0], out, piece, x0, y_half)
    export.export_glb(root, str(out / f"{piece}.glb"))
    export.write_export_report(str(out / "export-report.json"), root, piece, generator)
    bake.write_bake_report(str(out / "bake-report.json"), receiverSignature=bake.receiver_signature([lods[0]]),
                           device=os.environ.get("HEXW_BAKE_DEVICE", "CPU"), samples=64, marginPx=12,
                           aoDistanceM=1.0, channels={"masks": {"size": MASK_SIZE, "colorspace": "linear"}},
                           texelDensityPxPerM=texel, maskStats=stats, clearancePerLod=checks,
                           forestBaseSignature=sig, baseEdge=k, taper=list(taper), epsM=EPS, diveM=DIVE,
                           extM=EXT, depthM=D_END, skipped=skip_bake)


def bake_masks(low, out, piece, x0, y_half):
    """AO bake on LOD0 packed with forest_base's floor masks (snow 0.75, porosity 0.85, night 0)."""
    bake.setup_cycles(samples=64)
    scn = bpy.context.scene
    scn.world = scn.world or bpy.data.worlds.new(piece)
    scn.world.light_settings.distance = 1.0
    ao = bake.new_image(f"{piece}.ao", MASK_SIZE, non_color=True)
    bake.bake_to(low, "AO", ao, margin=12)
    ao_png = bake.save_png(ao, str(out / f"{piece}.ao.tmp.png"))
    masks_png = bake.make_masks(MASK_SIZE, 0.75, 0.85, 0.0, ao_png, str(out / f"{piece}.masks.png"))
    os.remove(ao_png)
    _, px = bake.load_png(masks_png)
    c = (np.arange(MASK_SIZE, dtype=np.float64) + 0.5) / MASK_SIZE
    us, vs = np.meshgrid(c, c)
    ys, xs = (us * 2 - 1) * y_half, x0 + vs * D_END
    reach = np.abs(ys) - xs * TAN30 - EXT / COS30
    inside = (reach < -0.05).ravel()
    # Texels the overlay never covers (corners of the UV rectangle) hold AO 1, not the bake's 0,
    # so deep mips do not darken the overlay's sides.
    px[(reach > 0.3).ravel(), 3] = 1.0
    bake.write_png(MASK_SIZE, px, masks_png, non_color=True)
    a = px[inside, 3]
    stats = {"texels": int(inside.sum()), "min": float(a.min()), "mean": float(a.mean()),
             "p01": float(np.percentile(a, 1)), "p05": float(np.percentile(a, 5))}
    print(f"{piece}: masks AO {stats}")
    return stats
