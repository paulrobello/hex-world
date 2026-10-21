"""forest_base: hex column (soil-strata skirt) + forest-floor cap. Scripted route (PRD §11.1).

A variant of meadow_base with the same border ring, seam contract, skirt, strata and pivot. The cap is
one mesh in three slots, split per face along two wavy contours at a seeded distance from the border:
- rim (`grass`, untinted, masks): the outer band, identical to meadow_base_cap, so a Forest-Grass
  seam has no joint (tileable sets sample in world space, so any change at the border would show);
- edge (`grass` tinted dark and mossy): a narrow transition band;
- floor (`litter`, scripts/blender/kit/mat_litter.py): leaf litter, needles and moss under the trees.
Both contours carry their own CDT points in every LOD, so the band exists at LOD1 and LOD2 too
(VisibilityRange is per chunk: a meadow at LOD1 can sit beside a forest at LOD2)."""
import math
import os
import pathlib
import random
import sys

import numpy as np

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
sys.path.insert(0, str(pathlib.Path(__file__).parent))
from lib import bake, collider, export, geom, hexgeo, lod, materials, scene, uv  # noqa: E402

import _terrain  # noqa: E402

PIECE = "forest_base"
SPACING = (0.9, 1.8, None)
CONTOUR_STEP = (0.7, 1.5, 4.0)  # contour point spacing per LOD (m along the contour)
SKIRT_ROWS = (4, 2, 1)
MASK_SIZE = 1024  # 32 px/m over the 32 m square (D2)
out = pathlib.Path(sys.argv[sys.argv.index("--") + 1])
skip_bake = os.environ.get("HEXW_SKIP_BAKE") == "1"
scene.reset_scene(PIECE)
random.seed(PIECE)


def band_rim(phi, m=math):
    """Distance from the border (m) where the grass rim ends, by polar angle: 1.05..3.35 m."""
    return 2.2 + 0.6 * m.sin(5 * phi + 1.3) + 0.35 * m.sin(11 * phi + 0.4) + 0.2 * m.sin(23 * phi + 2.1)


def band_floor(phi, m=math):
    """Where the mossy edge band ends and the litter floor starts: 0.6..1.2 m inside the rim."""
    return band_rim(phi, m) + 0.9 + 0.3 * m.sin(7 * phi + 0.7)


# The cap relief and its shading slope (`_terrain.py`, shared with _forest_edge.py): swell hollows and
# root-mound pits on a plane level with the border ring, z in [-0.25, 0] (owner D6-Q1). The swell is
# meadow_base's, so the rim matches the meadow's masks.
undulation = _terrain.undulation
relief = _terrain.forest_relief
shade_slope = _terrain.forest_shade_slope


def contour(band, step):
    """Points at distance band(phi) from the border, about `step` m apart along the contour."""
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
    """0 rim, 1 edge, 2 floor for a cap point (a face centroid)."""
    phi, d = math.atan2(y, x), hexgeo.dist_to_border(x, y)
    return 0 if d < band_rim(phi) else (1 if d < band_floor(phi) else 2)


root = scene.empty(PIECE)
slot_mats = [materials.pbr_material(f"{PIECE}_rim", materials.srgb("#4f7d32"), 0.9),
             materials.pbr_material(f"{PIECE}_edge", materials.srgb("#3d5a28"), 0.9),
             materials.pbr_material(f"{PIECE}_floor", materials.srgb("#5a3f26"), 0.9)]
skirt_mat = materials.pbr_material(f"{PIECE}_skirt", materials.srgb("#6b4a2f"), 0.95)
ring = hexgeo.border_ring(hexgeo.BORDER_SUBDIV)
lods, zone_faces = [], []
for i, (spacing, cstep, rows) in enumerate(zip(SPACING, CONTOUR_STEP, SKIRT_ROWS)):
    lines = contour(band_rim, cstep) + contour(band_floor, cstep)
    interior = hexgeo.lattice_points(spacing, spacing * 0.5) if spacing else [(0.0, 0.0)]
    # Clear lattice points close to a contour so CDT edges follow the contour, not a zigzag.
    keep = []
    for x, y in interior:
        phi, d = math.atan2(y, x), hexgeo.dist_to_border(x, y)
        if min(abs(d - band_rim(phi)), abs(d - band_floor(phi))) > 0.45 * (spacing or cstep):
            keep.append((x, y))
    cap = geom.cdt_cap(f"{PIECE}_lod{i}_cap", ring, keep + lines, relief)
    geom.set_cap_normals(cap, shade_slope)
    uv.planar_cap_uv(cap)
    for m_ in slot_mats:
        cap.data.materials.append(m_)
    counts = [0, 0, 0]
    for p in cap.data.polygons:
        c = p.center
        p.material_index = zone(c.x, c.y)
        counts[p.material_index] += 1
    zone_faces.append(counts)
    sk = geom.skirt(f"{PIECE}_lod{i}_skirt", ring, hexgeo.LEVEL_STEP, rows)
    sk.data.uv_layers.new(name="UVMap")
    for d in sk.data.uv_layers["UVMap"].data:
        d.uv = (0.005, 0.005)
    sk.data.materials.append(skirt_mat)
    body = geom.join([cap, sk], f"{PIECE}_lod{i}")
    lod.triangulate(body)
    body.parent = root
    lods.append(body)
lod.assert_border_identical(lods)
tris = [geom.tri_count(o) for o in lods]
assert tris[0] <= 4000 and tris[1] <= 0.5 * tris[0] and tris[2] <= 0.15 * tris[0], tris
# Every border-touching cap face must be rim (grass): the seam joint contract.
for o in lods:
    rim_idx = [k for k, m_ in enumerate(o.data.materials) if m_.name == f"{PIECE}_rim"][0]
    cap_idx = {k for k, m_ in enumerate(o.data.materials) if m_.name != f"{PIECE}_skirt"}
    for p in o.data.polygons:
        if p.material_index in cap_idx and any(abs(o.data.vertices[v].co.z) < 1e-6 and
                                               hexgeo.dist_to_border(*o.data.vertices[v].co.xy) < 1e-4
                                               for v in p.vertices):
            assert p.material_index == rim_idx, (o.name, p.index)
print(f"{PIECE}: tris per LOD {tris}, cap faces rim/edge/floor per LOD {zone_faces}")

col = collider.collider_from(lods[2], "hull", f"{PIECE}_collider")
col.parent = root
# Six prop anchors (seeded scatter-rules hook, PRD §6), as meadow_base: alternating inner/outer
# rings with a seeded angular jitter, at least 2.5 m inside the border, on the relief surface.
for n in range(6):
    a = math.radians(60 * n + 30 + random.uniform(-10.0, 10.0))
    r = (5.5 if n % 2 else 8.5) + random.uniform(-0.75, 0.75)
    x, y = r * math.cos(a), r * math.sin(a)
    assert hexgeo.dist_to_border(x, y) >= 2.5, (n, x, y)
    export.add_socket(f"anchor-prop-{n}", (x, y, relief(x, y)), root, kind="prop", radius=2.0)

texel = uv.measure_texel_density(lods[0], MASK_SIZE)
ao_stats = None
if not skip_bake:
    bake.setup_cycles(samples=64)
    ao = bake.new_image(f"{PIECE}.ao", MASK_SIZE, non_color=True)
    bake.bake_to(lods[0], "AO", ao, margin=12)
    ao_png = bake.save_png(ao, str(out / f"{PIECE}.ao.tmp.png"))
    # Texel centres in Blender pixel order (row 0 = V 0): x = 32U - 16, y = 32V - 16.
    c = (np.arange(MASK_SIZE, dtype=np.float64) + 0.5) / MASK_SIZE * 2 * hexgeo.HEX_R - hexgeo.HEX_R
    xs, ys = np.meshgrid(c, c)
    phi = np.arctan2(ys, xs)
    dist = hexgeo.APOTHEM - np.max([xs * math.cos(math.radians(-60 * k)) + ys * math.sin(math.radians(-60 * k))
                                    for k in range(6)], axis=0)
    # 0 on the rim, 1 on the floor, smooth across the edge band.
    t = np.clip((dist - band_rim(phi, np)) / np.maximum(band_floor(phi, np) - band_rim(phi, np), 1e-3), 0, 1)
    t = t * t * (3 - 2 * t)
    # Rim: meadow_base's masks exactly (snow 1, porosity 0.7 - 0.2 swell). Floor: humus holds water
    # (porosity 0.85) and the canopy keeps part of the snow off (capacity 0.75). Night 0.
    swell = undulation(xs, ys, np)
    wet = ((0.7 - 0.2 * swell) * (1 - t) + 0.85 * t).ravel().astype(np.float32)
    snow = (1.0 - 0.25 * t).ravel().astype(np.float32)
    masks_png = bake.make_masks(MASK_SIZE, snow, wet, 0.0, ao_png, str(out / f"{PIECE}.masks.png"))
    os.remove(ao_png)
    # AO over the hex footprint only (texels outside the hex are unbaked gutter).
    _, px = bake.load_png(masks_png)
    inside = (dist > 0.5).ravel()
    a = px[inside, 3]
    ao_stats = {"texels": int(inside.sum()), "min": float(a.min()), "mean": float(a.mean()),
                "p01": float(np.percentile(a, 1)), "snowMin": float(px[inside, 0].min()),
                "wetMin": float(px[inside, 1].min()), "wetMax": float(px[inside, 1].max())}
    print(f"{PIECE}: masks over the hex footprint {ao_stats}")
export.export_glb(root, str(out / f"{PIECE}.glb"))
export.write_export_report(str(out / "export-report.json"), root, PIECE, __file__)
bake.write_bake_report(str(out / "bake-report.json"), receiverSignature=bake.receiver_signature([lods[0]]),
                       device=os.environ.get("HEXW_BAKE_DEVICE", "CPU"), samples=64, marginPx=12,
                       channels={"masks": {"size": MASK_SIZE, "colorspace": "linear"}},
                       texelDensityPxPerM=texel, maskStats=ao_stats, capFacesPerSlot=zone_faces,
                       skipped=skip_bake)
