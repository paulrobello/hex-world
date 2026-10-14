"""tree_forest: a stylized-real Norway spruce, about 16 m, the forest tile's scatter tree (data/tiles/
forest.ron, density 3.0 per 100 m²). Scripted route, headless: make kit-build PIECE=tree_forest.

Three slots on one UV0 (geometry and wind in `_forest`):
- bark: the tileable `bark` set on the trunk's metre UVs (x BARK_UV_PER_M, `tileUvScale`, tinted
  toward spruce grey-red); its AO, baked from a displaced high-poly trunk, is in masks.A;
- core: the dark inner cone, colour only, sampling masks for an AO gradient (darker low and deep);
- needles: alpha-cutoff frond cards on the numpy-drawn frond tiles (rows v >= NEEDLE_V0), masks.A = 1.
The UV0 regions of the three masks-sampling slots never overlap. LOD1/LOD2 are regenerated from the
same branch list (fewer card segments, every 2nd card at LOD2), never decimated."""
import math
import os
import pathlib
import sys

import bmesh
import bpy
import numpy as np

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
from lib import bake, collider, export, geom, lod, materials, scene, uv  # noqa: E402
import _forest as F  # noqa: E402
import _tree  # noqa: E402

PIECE, ATLAS, MASK_SIZE = F.PIECE, F.ATLAS, 1024
BARK_SET_PX, BARK_TILE_M = 1024, 2.0  # the `bark` tileable set (scripts/blender/kit/mat_bark.py)
BARK_UV_PER_M = 1 / 64  # the 16.5 m trunk island fits below NEEDLE_V0 (0.258 < 0.281)
TILE_UV_SCALE = 1 / (BARK_UV_PER_M * BARK_TILE_M)  # manifest tree_forest_bark.tileUvScale = 32
BARK_BOX_U = 0.5  # the trunk island lives in u < 0.5, the core's masks box in u > 0.55
CORE_BOX = (0.55, 0.01, 0.98, 0.26)
CAGE, MAX_RAY, MARGIN, SAMPLES = 0.12, 0.3, 8, 32
AO_DIST = 0.3  # crevice scale (pitfalls: the 10 m world default darkens the whole trunk)
# Per LOD: trunk ring sides and ring step (m), core (sides, rings), main-card segments and fold,
# filler-card segments (None: no fillers) and fold, keep every n-th main card, width scale, leaders.
LODS = (dict(sides=6, step=1.4, core=(8, 6), segs=2, folded=True, fsegs=2, ffolded=True, every=1, width=1.0, leader=2),
        dict(sides=5, step=2.8, core=(6, 3), segs=2, folded=False, fsegs=1, ffolded=True, every=1, width=1.0, leader=2),
        dict(sides=4, step=5.5, core=(6, 2), segs=2, folded=False, fsegs=None, ffolded=False, every=2, width=1.3,
             leader=1))
out = pathlib.Path(sys.argv[sys.argv.index("--") + 1])
skip_bake = os.environ.get("HEXW_SKIP_BAKE") == "1"
scene.reset_scene(PIECE)
scn = bpy.context.scene
scn.world = scn.world or bpy.data.worlds.new(PIECE)
bark_mat = materials.pbr_material(f"{PIECE}_bark", materials.srgb("#6a4e3e"), 0.85)
needle_mat = materials.pbr_material(f"{PIECE}_needles", materials.srgb("#2e5226"), 0.65)
core_mat = materials.pbr_material(f"{PIECE}_core", materials.srgb("#0c180a"), 0.9)
skel = F.trunk_skeleton()
cards = F.branches()
mains = [b for b in cards if b["kind"] == "main"]
fillers = [b for b in cards if b["kind"] == "filler"]


def trunk(name, sides, step, uv_off=None):
    """The trunk tube with its UVMap scaled to atlas units and moved to uv_off (None: to the
    bottom-left of the atlas), then the wind UV. Returns (obj, uv_off)."""
    t = _tree.tube_mesh(name, skel, {0: sides}, {0: step})
    me = t.data
    a = np.empty(len(me.loops) * 2)
    me.uv_layers["UVMap"].data.foreach_get("uv", a)
    a *= BARK_UV_PER_M
    if uv_off is None:
        uv_off = (0.01 - a[0::2].min(), 0.005 - a[1::2].min())
    a[0::2] += uv_off[0]
    a[1::2] += uv_off[1]
    assert a.min() >= 0.0 and a[0::2].max() < BARK_BOX_U and a[1::2].max() < F.NEEDLE_V0 - 0.004, (a.min(), a.max())
    me.uv_layers["UVMap"].data.foreach_set("uv", a)
    _tree.bark_wind_uv(t)
    t.data.materials.append(bark_mat)
    return t, uv_off


lod_parts, uv_off = [], None
for i, c in enumerate(LODS):
    tr, uv_off = trunk(f"trunk_l{i}", c["sides"], c["step"], uv_off)
    core = F.core_mesh(f"core_l{i}", *c["core"], CORE_BOX)
    core.data.materials.append(core_mat)
    parts = [F.card_mesh(f"cards_l{i}", mains[::c["every"]], c["segs"], c["folded"], c["width"], c["leader"])]
    if c["fsegs"]:
        parts.append(F.card_mesh(f"fillers_l{i}", fillers, c["fsegs"], c["ffolded"], 1.0, leader=0))
    for p in parts:
        p.data.materials.append(needle_mat)
    lod_parts.append((tr, core, parts))

# Texel density per material on LOD0 (owner ruling): bark against its set through tileUvScale,
# needles against the atlas. The core is colour + masks only (not a UV0 density slot).
needles0 = geom.join(lod_parts[0][2], "needles_l0")
lod_parts[0] = (lod_parts[0][0], lod_parts[0][1], [needles0])
texel = {f"{PIECE}_bark": uv.measure_texel_density(lod_parts[0][0], TILE_UV_SCALE * BARK_SET_PX),
         f"{PIECE}_needles": uv.measure_texel_density(needles0, ATLAS)}
ao_density = uv.measure_texel_density(lod_parts[0][0], MASK_SIZE)
for name, t in texel.items():
    assert t["median"] >= 256, (name, t)

# Frond strip, atlas maps and masks.
strip_alb, strip_nrm, strip_rough = F.frond_strip()
assert strip_alb.shape[:2] == (ATLAS - F.NEEDLE_ROWS0, ATLAS), strip_alb.shape
coverage = F.coverage_by_mip(strip_alb[..., 3])
albedo = np.empty((ATLAS, ATLAS, 4), np.float32)
albedo[:] = (*strip_alb[..., :3][strip_alb[..., 3] > 0.5].mean(0), 1.0)
albedo[F.NEEDLE_ROWS0:] = strip_alb
normal = np.empty((ATLAS, ATLAS, 4), np.float32)
normal[:] = (0.5, 0.5, 1.0, 1.0)
normal[F.NEEDLE_ROWS0:, :, :3] = strip_nrm
orm = np.empty((ATLAS, ATLAS, 4), np.float32)
orm[:] = (1.0, 0.65, 0.0, 1.0)
orm[F.NEEDLE_ROWS0:, :, 1] = strip_rough

# masks: R snow capacity (needles hold snow, bark little), G porosity, B night 0, A AO.
mk = np.empty((MASK_SIZE, MASK_SIZE, 4), np.float32)
mk[:] = (1.0, 0.5, 0.0, 1.0)
cu0, cv0, cu1, cv1 = (int(round(x * MASK_SIZE)) for x in CORE_BOX)
rows = np.linspace(0.0, 1.0, cv1 - cv0)[:, None]
mk[cv0:cv1, cu0:cu1, 3] = 0.5 + 0.35 * rows  # the core: darker low in the crown, lighter up top
mk[cv0:cv1, cu0:cu1, 0] = 0.6
bake_stats = {"needleAlphaCoverageByMip": coverage}


def island_texels(obj, size):
    """obj's UV0 islands rasterized at texel centres (bool size x size, Blender pixel order)."""
    island = np.zeros((size, size), bool)
    uvd = obj.data.uv_layers["UVMap"].data
    for poly in obj.data.polygons:
        loops = list(poly.loop_indices)
        for k in range(1, len(loops) - 1):
            tri = np.array([uvd[loops[0]].uv[:], uvd[loops[k]].uv[:], uvd[loops[k + 1]].uv[:]]) * size
            lo_, hi_ = np.floor(tri.min(0)).astype(int), np.ceil(tri.max(0)).astype(int)
            xs, ys = np.meshgrid(np.arange(lo_[0], hi_[0]) + 0.5, np.arange(lo_[1], hi_[1]) + 0.5)
            (ax, ay), (bx, by), (cx, cy) = tri
            d = (by - cy) * (ax - cx) + (cx - bx) * (ay - cy)
            if abs(d) < 1e-12:
                continue
            l1 = ((by - cy) * (xs - cx) + (cx - bx) * (ys - cy)) / d
            l2 = ((cy - ay) * (xs - cx) + (ax - cx) * (ys - cy)) / d
            inside = (l1 >= 0) & (l2 >= 0) & (l1 + l2 <= 1)
            island[ys[inside].astype(int).clip(0, size - 1), xs[inside].astype(int).clip(0, size - 1)] = True
    return island


trunk0 = lod_parts[0][0]
isl = island_texels(trunk0, MASK_SIZE)
grow = isl.copy()
for _ in range(MARGIN):
    grow = grow | np.roll(grow, 1, 0) | np.roll(grow, -1, 0) | np.roll(grow, 1, 1) | np.roll(grow, -1, 1)
mk[..., 0][grow] = 0.3
if not skip_bake:
    bark_high = _tree.tube_mesh("bark_high", skel, {0: 16}, {0: 0.25}, displace=0.035)
    bake.setup_cycles(samples=SAMPLES)
    scn.world.light_settings.distance = AO_DIST
    ao_img = bake.new_image(f"{PIECE}.ao", MASK_SIZE, non_color=True)
    bake.bake_to(trunk0, "AO", ao_img, highs=[bark_high], cage_extrusion=CAGE, max_ray=MAX_RAY, margin=MARGIN)
    ao_tmp = bake.save_png(ao_img, str(out / f"{PIECE}.ao.tmp.png"))
    _, ao_px = bake.load_png(ao_tmp)
    os.remove(ao_tmp)
    ao = ao_px[:, 0].reshape(MASK_SIZE, MASK_SIZE).copy()
    # The relief AO of a lone trunk is near 1 (bark crevices only). Add the occlusion the bake leaves
    # out on purpose (pitfalls: other meshes stay hidden): the ground contact at the base and the
    # crown's shade above the lowest whorl, analytic in height. Island V is metres along the trunk.
    v = (np.arange(MASK_SIZE) + 0.5) / MASK_SIZE
    z = (v - uv_off[1]) / BARK_UV_PER_M - skel[0]["radii"][0] * 0.3 - 0.15
    s1 = np.clip(z / 1.2, 0, 1)
    s2 = np.clip((z - (F.Z0 - 0.5)) / 1.0, 0, 1)
    shade = (0.72 + 0.28 * s1 * s1 * (3 - 2 * s1)) * (1 - 0.15 * s2 * s2 * (3 - 2 * s2))
    ao *= shade[:, None]
    mk[..., 3] = np.where(grow, ao, mk[..., 3])
    a = ao[isl]
    bake_stats["barkAo"] = {"texels": int(a.size), "min": float(a.min()), "mean": float(a.mean()),
                            "p01": float(np.percentile(a, 1)), "p05": float(np.percentile(a, 5))}
    bpy.data.objects.remove(bark_high, do_unlink=True)
bake.write_png(MASK_SIZE, mk.reshape(-1, 4), str(out / f"{PIECE}.masks.png"))
bake.write_png(ATLAS, albedo.reshape(-1, 4), str(out / f"{PIECE}.albedo.png"), non_color=False)
bake.write_png(ATLAS, normal.reshape(-1, 4), str(out / f"{PIECE}.normal.png"))
bake.write_png(ATLAS, orm.reshape(-1, 4), str(out / f"{PIECE}.orm.png"))
print(f"{PIECE}: bake stats {bake_stats}")

# Join each LOD; wind clamp after the join; crown normals last.
lods = []
for i, (tr, core, parts) in enumerate(lod_parts):
    o = geom.join([tr, core, *parts], f"{PIECE}_lod{i}")
    lod.triangulate(o)
    _tree.clamp_wind_uv(o)
    F.set_spruce_normals(o, needle_mat.name, core_mat.name)
    lods.append(o)
tris = [geom.tri_count(o) for o in lods]
assert tris[0] <= 3000 and tris[1] <= 0.5 * tris[0] and tris[2] <= 0.15 * tris[0], tris
zmin = min(v.co.z for o in lods for p in o.data.polygons
           if o.data.materials[p.material_index].name == needle_mat.name for v in (o.data.vertices[k] for k in p.vertices))
assert zmin > 0.6, zmin  # cards stay clear of the gate's 0.5 m still band
print(f"{PIECE}: tris per LOD {tris}, cards {len(mains)} main + {len(fillers)} filler, lowest card z {zmin:.2f}, "
      f"texel {texel}, bark AO texels {ao_density}")

# Collider: hull of the lowest 3 m of trunk (a stroll-blocker, D14), 8 points per level, staggered.
ring_pts = []
for zi, z in enumerate((0.0, 1.0, 2.0, 3.0)):
    p = min(skel[0]["points"], key=lambda q: abs(q.z - z))
    r = skel[0]["radii"][skel[0]["points"].index(p)]
    ring_pts += [(p.x + r * math.cos(2 * math.pi * (k + 0.5 * zi) / 8), p.y + r * math.sin(2 * math.pi * (k + 0.5 * zi) / 8), z)
                 for k in range(8)]
cbm = bmesh.new()
for q in ring_pts:
    cbm.verts.new(q)
bmesh.ops.convex_hull(cbm, input=cbm.verts)
cme = bpy.data.meshes.new("collider_src")
cbm.to_mesh(cme)
cbm.free()
csrc = scene.link(bpy.data.objects.new("collider_src", cme))
col = collider.collider_from(csrc, "hull", f"{PIECE}_collider")
bpy.data.objects.remove(csrc, do_unlink=True)

root = scene.empty(PIECE)
for o in lods:
    o.parent = root
col.parent = root
sig = bake.receiver_signature([lods[0]])
export.export_glb(root, str(out / f"{PIECE}.glb"))
export.write_export_report(str(out / "export-report.json"), root, PIECE, __file__, budget_class="piece")
bake.write_bake_report(str(out / "bake-report.json"), receiverSignature=sig, samples=SAMPLES, marginPx=MARGIN,
                       cageExtrusionM=CAGE, maxRayDistanceM=MAX_RAY, aoDistanceM=AO_DIST, texelDensityPxPerM=texel,
                       barkTile={"set": "bark", "tileUvScale": TILE_UV_SCALE, "uvPerM": BARK_UV_PER_M,
                                 "aoTexelDensityPxPerM": ao_density},
                       cards={"main": len(mains), "filler": len(fillers), "frondTiles": F.FROND_TILES},
                       channels={"albedo": {"size": ATLAS, "colorspace": "srgb"},
                                 "normal": {"size": ATLAS, "colorspace": "linear"},
                                 "orm": {"size": ATLAS, "colorspace": "linear"},
                                 "masks": {"size": MASK_SIZE, "colorspace": "linear"}},
                       device=os.environ.get("HEXW_BAKE_DEVICE", "CPU"), bakeStats=bake_stats, skipped=skip_bake)
