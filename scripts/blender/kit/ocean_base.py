"""ocean_base: the ocean tile's column, a sandy seabed at the contract bed level. Scripted route.
Bed contract (docs/kit/water-contract.md §4, §6): flat at y = -4.0 on every edge, interior relief
rises at most 0.10 m and is 0 within 2 m of the border, checked per LOD against the profile. The bed
is the column floor, so there is no strata skirt: the engine draws the water's cut face on an
exposed edge. Border ring identical in every LOD; never decimated."""
import os
import pathlib
import sys

import bpy
import numpy as np
from mathutils import Matrix

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
from lib import bake, collider, export, geom, hexgeo, lod, materials, scene, uv  # noqa: E402
import _coast  # noqa: E402

PIECE = "ocean_base"
SPACING = (1.2, 2.4, None)
HIGH_SPACING = 0.15  # resolves the 0.85 m ripples for the AO bake
MASK_SIZE = 1024  # 32 px/m over the 32 m square (D2)
AO_DISTANCE_M = 0.6  # local relief scale: pebbles, rocks, ripples
CAGE_M, MAX_RAY_M = 0.3, 0.6
out = pathlib.Path(sys.argv[sys.argv.index("--") + 1])
skip_bake = os.environ.get("HEXW_SKIP_BAKE") == "1"
scene.reset_scene(PIECE)
scn = bpy.context.scene
scn.world = scn.world or bpy.data.worlds.new(PIECE)
scn.world.light_settings.distance = AO_DISTANCE_M
DROP = Matrix.Translation((0.0, 0.0, _coast.OCEAN_BED_Y))


def cap(name, spacing, height_fn):
    """Cap built at z = 0 (where the lib's border helpers work), then its vertex data dropped to the
    bed. The drop is in the mesh, not a node transform: the gate reads raw accessor positions."""
    interior = hexgeo.lattice_points(spacing, spacing * 0.5) if spacing else []
    obj = geom.cdt_cap(name, ring, interior, height_fn)
    obj.data.name = name
    geom.set_border_normals_up(obj)
    return obj


root = scene.empty(PIECE)
sand = materials.pbr_material("coast_seabed", materials.srgb("#a39579"), 0.85)
ring = hexgeo.border_ring(hexgeo.BORDER_SUBDIV)
lods = []
for i, spacing in enumerate(SPACING):
    body = cap(f"{PIECE}_lod{i}", spacing, _coast.relief)
    uv.planar_cap_uv(body)
    body.data.materials.append(sand)
    lod.triangulate(body)
    lods.append(body)
lod.assert_border_identical(lods)
for body in lods:
    body.data.transform(DROP)
    body.data.update()
    body.parent = root
bad = [m for body in lods for m in _coast.bed_profile_errors(body)]
assert not bad, "bed off its profile:\n" + "\n".join(bad[:20])
tris = [geom.tri_count(o) for o in lods]
assert tris[0] <= 4000 and tris[1] <= 0.5 * tris[0] and tris[2] <= 0.15 * tris[0], tris
print(f"{PIECE}: tris per LOD {tris}")

col = collider.collider_from(lods[2], "cap", f"{PIECE}_collider")
col.parent = root

texel = uv.measure_texel_density(lods[0], MASK_SIZE)
mask_stats = None
bake_meta = {}
if not skip_bake:
    # AO source: the rippled high-poly bed plus the pebbles and rocks ocean_center lays on it, so
    # the masks carry ripple troughs and contact shadows. None of these meshes is exported.
    high = cap(f"{PIECE}_high", HIGH_SPACING, _coast.ripples)
    high.data.transform(DROP)
    assert not _coast.bed_profile_errors(high), "high-poly bed off its profile"
    stones = [_coast.stone(f"{PIECE}_high_stone{n}", it, 2) for n, it in enumerate(_coast.scatter())]
    bake.setup_cycles(samples=64)
    ao = bake.new_image(f"{PIECE}.ao", MASK_SIZE, non_color=True)
    bake.bake_to(lods[0], "AO", ao, highs=[high, *stones], cage_extrusion=CAGE_M, max_ray=MAX_RAY_M, margin=12)
    ao_png = bake.save_png(ao, str(out / f"{PIECE}.ao.tmp.png"))
    bake_meta = {"highTris": geom.tri_count(high) + sum(geom.tri_count(s) for s in stones),
                 "aoDistanceM": AO_DISTANCE_M, "cageExtrusionM": CAGE_M, "maxRayDistanceM": MAX_RAY_M}
    for o in [high, *stones]:
        bpy.data.objects.remove(o)
    bpy.context.view_layer.update()
    # Underwater: no snow capacity (R), no rain wetting (G, the bed is wet already), no lights (B).
    masks_png = bake.make_masks(MASK_SIZE, 0.0, 0.0, 0.0, ao_png, str(out / f"{PIECE}.masks.png"))
    os.remove(ao_png)
    # AO over the hex footprint only (texels outside the hex are unbaked gutter).
    c = (np.arange(MASK_SIZE, dtype=np.float64) + 0.5) / MASK_SIZE * 2 * hexgeo.HEX_R - hexgeo.HEX_R
    xs, ys = np.meshgrid(c, c)
    _, px = bake.load_png(masks_png)
    inside = (np.max([xs * np.cos(np.radians(-60 * k)) + ys * np.sin(np.radians(-60 * k))
                      for k in range(6)], axis=0) < hexgeo.APOTHEM - 0.5).ravel()
    a = px[inside, 3]
    mask_stats = {"texels": int(inside.sum()), "min": float(a.min()), "mean": float(a.mean()),
                  "p01": float(np.percentile(a, 1)), "p05": float(np.percentile(a, 5)),
                  "missRate": float((a <= 0.0).mean())}
    print(f"{PIECE}: masks AO over the hex footprint {mask_stats}")
export.export_glb(root, str(out / f"{PIECE}.glb"))
export.write_export_report(str(out / "export-report.json"), root, PIECE, __file__)
bake.write_bake_report(str(out / "bake-report.json"), receiverSignature=bake.receiver_signature([lods[0]]),
                       device=os.environ.get("HEXW_BAKE_DEVICE", "CPU"), samples=64, marginPx=12,
                       channels={"masks": {"size": MASK_SIZE, "colorspace": "linear"}},
                       bedY=_coast.OCEAN_BED_Y, surfaceY=_coast.SURFACE_Y, oceanDepthM=_coast.OCEAN_DEPTH,
                       texelDensityPxPerM=texel, maskStats=mask_stats, skipped=skip_bake, **bake_meta)
