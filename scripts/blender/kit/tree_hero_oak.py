"""tree_hero_oak export stage: opens the live-authored high-poly .blend (R1: local, hashed) and runs
the shared low/bake/LOD/export pipeline headless. Usage: make kit-build PIECE=tree_hero_oak.

Three materials on one UV0 (owner: solid-dome canopy):
- Canopy: one closed low-poly R0 dome shell (`_tree.dome_sector`, SECTORS-fold, every sector on the
  same UV island set) packed below v = _tree.LEAF_V0 and baked selected-to-active from the leaf-covered
  master dome `tree_hero_oak_canopy_high` (albedo through EMIT, normal, AO into ORM.R). Every clump is
  a transformed copy of that shell, so they share the bake. Slot `unique`, no masks.
- Bark: the tileable `bark` set mapped on UV0 (metres x BARK_UV_PER_M, `tileUvScale`); its AO is in
  masks.A, on the same UV0 (the canopy slot does not sample masks, so the two may overlap in UV).
- Fringe leaves: alpha cards on the unique strip v >= LEAF_V0 (`_leaves`), masks.A = 1 there."""
import math
import os
import pathlib
import sys

import bmesh
import bpy
import numpy as np

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
from lib import bake, collider, export, geom, lod, paths, scene, uv  # noqa: E402
import _leaves  # noqa: E402
import _tree  # noqa: E402

PIECE, ATLAS, MASK_SIZE = "tree_hero_oak", 2048, 1024
BARK_SET_PX, BARK_TILE_M = 1024, 2.0  # the `bark` tileable set (scripts/blender/kit/mat_bark.py)
BARK_UV_PER_M = 1 / 24  # atlas UV per metre of bark: AO at MASK_SIZE / 24 = 42.7 px/m
TILE_UV_SCALE = 1 / (BARK_UV_PER_M * BARK_TILE_M)  # manifest tree_hero_oak_bark.tileUvScale = 12
BLEND = pathlib.Path(os.environ.get("HEXW_BLEND") or paths.blend_path(PIECE))  # main-checkout store (O2)
CAGE, MAX_RAY, MARGIN, SAMPLES = 0.25, 0.5, 8, 32  # bark: cage clears the 6-sided receiver
SHELL_CAGE, SHELL_RAY = 0.45, 0.9  # canopy: clears leaves stacked up to ~0.25 m off the dome
# AO ray lengths: crevice scale. The world default (10 m) lets limbs 4 m up darken the whole trunk.
AO_DIST, SHELL_AO_DIST = 0.35, 0.3
CANOPY_MAT = f"{PIECE}_canopy"
out = pathlib.Path(sys.argv[sys.argv.index("--") + 1])
skip_bake = os.environ.get("HEXW_SKIP_BAKE") == "1"

bpy.ops.wm.open_mainfile(filepath=str(BLEND))
scn = bpy.data.scenes[PIECE]
assert bpy.context.scene == scn, bpy.context.scene
bpy.context.preferences.filepaths.save_version = 0
HIGH_NAMES = (f"{PIECE}_bark_high", f"{PIECE}_leaves_high", f"{PIECE}_canopy_high")
# The showcase rig (lights, camera, ground, canopy instances) is not a bake input and never enters
# the export root.
for o in list(scn.objects):
    if o.name not in HIGH_NAMES:
        bpy.data.objects.remove(o, do_unlink=True)
bpy.context.view_layer.update()
bark_high, leaves_high, canopy_high = (bpy.data.objects[n] for n in HIGH_NAMES)
canopy_high.hide_render = False  # hidden in the live showcase (instances show it), a bake source here
canopy_high.hide_set(False)
bark_mat, leaf_mat = bpy.data.materials[f"{PIECE}_bark"], bpy.data.materials[f"{PIECE}_leaves"]
canopy_mat = bpy.data.materials.get(CANOPY_MAT) or bpy.data.materials.new(CANOPY_MAT)
skel = _tree.skeleton()
crown = _tree.layout()
assert _tree.skeleton_sha(skel) == scn["tho_skeleton_sha"], "skeleton code changed since the .blend was saved"


def pack_below_strip(obj, rotate, scale, margin):
    """Pack obj's UVMap islands into v < LEAF_V0 around a locked placeholder over the leaf strip."""
    ph_mesh = bpy.data.meshes.new("leaf_strip_placeholder")
    ph_mesh.from_pydata([(0, 0, -90), (1, 0, -90), (1, 1, -90), (0, 1, -90)], [], [[0, 1, 2, 3]])
    ph = scene.link(bpy.data.objects.new("leaf_strip_placeholder", ph_mesh))
    ph_uv = ph_mesh.uv_layers.new(name="UVMap")
    for loop, co in zip(ph_mesh.loops, ((0, _tree.LEAF_V0), (1, _tree.LEAF_V0), (1, 1), (0, 1))):
        ph_uv.data[loop.index].uv = co
    # Blender 5.x stores UV pins as the boolean corner attribute ".pn.<uv layer>".
    pin = ph_mesh.attributes.get(".pn.UVMap") or ph_mesh.attributes.new(".pn.UVMap", "BOOLEAN", "CORNER")
    pin.data.foreach_set("value", [True] * len(ph_mesh.loops))
    scene.select_only([obj, ph], obj)
    bpy.ops.object.mode_set(mode="EDIT")
    bpy.ops.mesh.select_all(action="SELECT")
    bpy.ops.uv.select_all(action="SELECT")
    bpy.ops.uv.pack_islands(rotate=rotate, scale=scale, margin=margin, pin=True, pin_method="LOCKED")
    bpy.ops.object.mode_set(mode="OBJECT")
    bpy.data.objects.remove(ph, do_unlink=True)
    a = np.empty(len(obj.data.loops) * 2)
    obj.data.uv_layers["UVMap"].data.foreach_get("uv", a)
    vmax = float(a[1::2].max())
    assert vmax < _tree.LEAF_V0 and a.min() >= 0.0 and a.max() <= 1.0, (obj.name, a.min(), vmax, a.max())
    return vmax


# LOD0 bark receiver: the same skeleton at low side counts, closed tubes; faces buried in another
# branch or hidden inside a canopy dome are culled. (branches, level, ring sides, ring step m).
trunk = skel[:3]
roots = [b for b in skel[3:] if b["level"] == 0]
limbs = [b for b in skel if b["level"] == 1]
twigs = [b for b in skel if b["level"] == 2]
groups = ((trunk, 0, 6, 0.6), (roots, 0, 4, 1.0), (limbs, 1, 5, 1.5), (twigs, 2, 4, 1.5))
parts = [_tree.tube_mesh(f"bark_{i}", bs, {lv: n}, {lv: st}) for i, (bs, lv, n, st) in enumerate(groups)]
bark_low = geom.join(parts, "bark_low")
faces_before = len(bark_low.data.polygons)
culled = _tree.cull_buried(bark_low, skel, {b["id"]: n for bs, _, n, _ in groups for b in bs})
culled_crown = _tree.cull_inside_crown(bark_low, crown)
print(f"{PIECE}: culled {culled} buried and {culled_crown} in-crown of {faces_before} bark faces")
bark_low.data.materials.append(bark_mat)
lod.triangulate(bark_low)
me = bark_low.data
bv = np.empty(len(me.loops) * 2)
me.uv_layers["UVMap"].data.foreach_get("uv", bv)
me.uv_layers["UVMap"].data.foreach_set("uv", bv * BARK_UV_PER_M)
# Unrotated and unscaled, so the grain stays along every branch and tileUvScale is exact.
bark_vmax = pack_below_strip(bark_low, rotate=False, scale=False, margin=0.003)
_tree.bark_wind_uv(bark_low)  # TEXCOORD_1 after the atlas pack, so UVMap stays first and active

# Canopy shell: one sector unwrapped and packed below the strip, then closed by rotation (the other
# sectors reuse its UVs, and the SECTORS-fold master bakes identically into every sector).
shell = _tree.dome_sector("shell_base")
uv.atlas_unwrap([shell], margin=0.01)
shell_vmax = pack_below_strip(shell, rotate=True, scale=True, margin=0.01)
_tree.replicate_sectors(shell)
shell.data.materials.append(canopy_mat)
lod.triangulate(shell)
_tree.shell_smooth_normals(shell)  # before the bake: the normal map then holds leaf relief only

# LOD0 fringe: the live-authored cards (UVs already on the leaf strip).
leaves_low = leaves_high.copy()
leaves_low.data = leaves_high.data.copy()
leaves_low.name = leaves_low.data.name = "leaves_low"
scene.link(leaves_low)
_tree.leaf_wind_uv(leaves_low, skel)  # before the LOD subsets, which inherit it

# Bakes: the shell from the master dome (both at the origin), bark AO into masks.
strip_px = ATLAS - int(round(_tree.LEAF_V0 * ATLAS))
leaf_alb, leaf_nrm, leaf_rough = _leaves.strip(strip_px, _tree.SEED)
assert leaf_alb.shape[1] == ATLAS, leaf_alb.shape
albedo = np.empty((ATLAS, ATLAS, 4), np.float32)
albedo[:] = (*leaf_alb[..., :3].reshape(-1, 3).mean(0), 1.0)
normal = np.empty((ATLAS, ATLAS, 4), np.float32)
normal[:] = (0.5, 0.5, 1.0, 1.0)
orm = np.empty((ATLAS, ATLAS, 4), np.float32)
orm[:] = (1.0, 0.62, 0.0, 1.0)
bake_stats = {}
masks_path = str(out / f"{PIECE}.masks.png")
if not skip_bake:
    bake.setup_cycles(samples=SAMPLES)
    kw = dict(cage_extrusion=SHELL_CAGE, max_ray=SHELL_RAY, margin=MARGIN)
    imgs = {k: bake.new_image(f"{PIECE}.shell_{k}", ATLAS, non_color=k != "albedo")
            for k in ("albedo", "normal", "ao")}
    bake.bake_emit_input([canopy_high], shell, "Base Color", imgs["albedo"], **kw)
    bake.bake_to(shell, "NORMAL", imgs["normal"], highs=[canopy_high], **kw)
    scn.world.light_settings.distance = SHELL_AO_DIST
    bake.bake_to(shell, "AO", imgs["ao"], highs=[canopy_high], **kw)
    px = {}
    for k, im in imgs.items():
        p = bake.save_png(im, str(out / f"{PIECE}.shell_{k}.tmp.png"))
        px[k] = bake._load(p)[1].reshape(ATLAS, ATLAS, 4)
        os.remove(p)
    albedo[:] = px["albedo"]
    albedo[..., 3] = 1.0
    normal[:] = px["normal"]
    normal[..., 3] = 1.0
    orm[..., 0] = px["ao"][..., 0]
    # Bark AO -> masks.A at MASK_SIZE on UV0 (selected-to-active from the bark high poly).
    scn.world.light_settings.distance = AO_DIST
    ao_img = bake.new_image(f"{PIECE}.ao", MASK_SIZE, non_color=True)
    bake.bake_to(bark_low, "AO", ao_img, highs=[bark_high], cage_extrusion=CAGE, max_ray=MAX_RAY, margin=MARGIN)
    ao_tmp = bake.save_png(ao_img, str(out / f"{PIECE}.ao.tmp.png"))
    _, ao_px = bake._load(ao_tmp)
    os.remove(ao_tmp)
    ao = ao_px[:, 0].reshape(MASK_SIZE, MASK_SIZE).copy()
    ao[int(round(_tree.LEAF_V0 * MASK_SIZE)):] = 1.0  # fringe leaves: no baked AO
    bake.make_masks(MASK_SIZE, 1.0, 0.5, 0.0, None, masks_path)
    _, mk = bake._load(masks_path)
    mk[:, 3] = ao.ravel()
    bake._write(MASK_SIZE, mk, masks_path)

    def island_texels(obj, size):
        """The obj's UV0 islands rasterized at texel centres (bool size x size, Blender pixel order)."""
        island = np.zeros((size, size), bool)
        uvd = obj.data.uv_layers["UVMap"].data
        for poly in obj.data.polygons:
            tri = np.array([uvd[li].uv[:] for li in poly.loop_indices]) * size
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

    def stats(a):
        return {"texels": int(a.size), "min": float(a.min()), "mean": float(a.mean()),
                "p01": float(np.percentile(a, 1)), "p05": float(np.percentile(a, 5))}

    bake_stats["barkAo"] = stats(ao[island_texels(bark_low, MASK_SIZE)])
    bake_stats["canopyAo"] = stats(orm[..., 0][island_texels(shell, ATLAS)])
else:
    bake.make_masks(MASK_SIZE, 1.0, 0.5, 0.0, None, masks_path)
albedo[-strip_px:] = leaf_alb
normal[-strip_px:, :, :3] = leaf_nrm
orm[-strip_px:, :, 0] = 1.0
orm[-strip_px:, :, 1] = leaf_rough
bake._write(ATLAS, albedo.reshape(-1, 4), str(out / f"{PIECE}.albedo.png"), non_color=False)
bake._write(ATLAS, normal.reshape(-1, 4), str(out / f"{PIECE}.normal.png"))
bake._write(ATLAS, orm.reshape(-1, 4), str(out / f"{PIECE}.orm.png"))
cov, m = [], leaf_alb[..., 3]
while min(m.shape) >= 1:
    cov.append(round(float((m > 0.5).mean()), 4))
    if min(m.shape) < 2:
        break
    h2, w2 = m.shape[0] // 2 * 2, m.shape[1] // 2 * 2
    m = m[:h2, :w2].reshape(h2 // 2, 2, w2 // 2, 2).mean(axis=(1, 3))
bake_stats["leafAlphaCoverageByMip"] = cov

# Canopy LOD0: one transformed shell per clump (shared bake), with its wind UV.
shells = []
for cl in crown:
    o = shell.copy()
    o.data = shell.data.copy()
    o.name = o.data.name = f"shell_{cl['id']:02d}"
    o.matrix_world = _tree.clump_matrix(cl)
    scene.link(o)
    _tree.shell_wind_uv(o, cl, skel)
    shells.append(o)
canopy_low = geom.join(shells, "canopy_low")
bpy.data.objects.remove(shell, do_unlink=True)
print(f"{PIECE}: bake stats {bake_stats}")


def card_subset(src, every, scale, name):
    """Keep every `every`-th card, scaled about its centre to hold canopy coverage (LOD1/LOD2)."""
    bm = bmesh.new()
    bm.from_mesh(src.data)
    bm.faces.ensure_lookup_table()
    drop = [f for i, f in enumerate(bm.faces) if i % every]
    keep = [f for i, f in enumerate(bm.faces) if not i % every]
    for f in keep:
        c = f.calc_center_median()
        for v in f.verts:
            v.co = c + (v.co - c) * scale
    bmesh.ops.delete(bm, geom=drop, context="FACES")
    mesh = bpy.data.meshes.new(name)
    bm.to_mesh(mesh)
    bm.free()
    for m_ in src.data.materials:
        mesh.materials.append(m_)
    return scene.link(bpy.data.objects.new(name, mesh))


def copy_of(src, name):
    o = src.copy()
    o.data = src.data.copy()
    o.name = o.data.name = name
    return scene.link(o)


lod.triangulate(leaves_low)
leaves1 = card_subset(leaves_low, 2, 1.25, "leaves_l1")
leaves2 = card_subset(leaves_low, 4, 1.5, "leaves_l2")
for o in (leaves1, leaves2):
    lod.triangulate(o)
bark1, bark2 = copy_of(bark_low, "bark_l1"), copy_of(bark_low, "bark_l2")
canopy1, canopy2 = copy_of(canopy_low, "canopy_l1"), copy_of(canopy_low, "canopy_l2")

# Collider: hull of the trunk strands' lowest 4 m (a stroll-blocker, D14), 8 points per metre level.
ring_pts = []
for zi, z in enumerate((0.0, 1.0, 2.0, 3.0, 3.6)):
    rad = 0.0
    for b in trunk:
        for p, r in zip(b["points"], b["radii"]):
            if abs(p.z - z) < 0.3:
                rad = max(rad, math.hypot(p.x, p.y) + r)
    # Staggered rings (half a step per level) keep the hull free of coplanar side quads.
    ring_pts += [(rad * math.cos(2 * math.pi * (k + 0.5 * zi) / 8), rad * math.sin(2 * math.pi * (k + 0.5 * zi) / 8), z)
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
assert len(col.data.vertices) <= 64, len(col.data.vertices)

# Texel density per material (owner ruling): bark against its tileable set through tileUvScale,
# canopy and fringe against the unique atlas. Each median must reach 256 px/m on its own.
texel = {f"{PIECE}_bark": uv.measure_texel_density(bark_low, TILE_UV_SCALE * BARK_SET_PX),
         CANOPY_MAT: uv.measure_texel_density(canopy_low, ATLAS),
         f"{PIECE}_leaves": uv.measure_texel_density(leaves_low, ATLAS)}
ao_density = uv.measure_texel_density(bark_low, MASK_SIZE)

# LODs: canopy shells decimate; fringe keeps card subsets; bark takes the rest of each budget.
lods = []
parts0 = [geom.tri_count(o) for o in (bark_low, canopy_low, leaves_low)]
t0 = sum(parts0)
for i, (b, c, lv) in enumerate(((bark_low, canopy_low, leaves_low), (bark1, canopy1, leaves1),
                                (bark2, canopy2, leaves2))):
    if i:
        budget = int(t0 * (0.5 if i == 1 else 0.15)) - geom.tri_count(lv) - 4
        lod.decimate_to(c, int(budget * (0.55 if i == 1 else 0.6)))
        lod.decimate_to(b, budget - geom.tri_count(c))
    o = geom.join([b, c, lv], f"{PIECE}_lod{i}")
    _tree.clamp_wind_uv(o)
    _tree.set_canopy_normals(o, leaf_mat.name, CANOPY_MAT)
    lods.append(o)
tris = [geom.tri_count(o) for o in lods]
assert tris[0] <= 3000 and tris[1] <= 0.5 * tris[0] and tris[2] <= 0.15 * tris[0], tris
print(f"{PIECE}: tris per LOD {tris} (LOD0 bark/canopy/fringe {parts0}), texel {texel}, "
      f"bark AO texels {ao_density}, bark v max {bark_vmax:.4f}, shell v max {shell_vmax:.4f}")
for name, t in texel.items():
    assert t["median"] >= 256, (name, t)
sig = bake.receiver_signature([lods[0]])

root = scene.empty(PIECE)
for o in lods:
    o.parent = root
col.parent = root
export.export_glb(root, str(out / f"{PIECE}.glb"))
export.write_export_report(str(out / "export-report.json"), root, PIECE, __file__, budget_class="hero")
bake.write_bake_report(str(out / "bake-report.json"), receiverSignature=sig, samples=SAMPLES, marginPx=MARGIN,
                       cageExtrusionM=CAGE, maxRayDistanceM=MAX_RAY, texelDensityPxPerM=texel,
                       canopyShell={"cageExtrusionM": SHELL_CAGE, "maxRayDistanceM": SHELL_RAY,
                                    "aoDistanceM": SHELL_AO_DIST, "sectors": _tree.SECTORS,
                                    "clumps": len(crown)},
                       barkTile={"set": "bark", "tileUvScale": TILE_UV_SCALE, "uvPerM": BARK_UV_PER_M,
                                 "aoTexelDensityPxPerM": ao_density},
                       channels={"albedo": {"size": ATLAS, "colorspace": "srgb"},
                                 "normal": {"size": ATLAS, "colorspace": "linear"},
                                 "orm": {"size": ATLAS, "colorspace": "linear"},
                                 "masks": {"size": MASK_SIZE, "colorspace": "linear"}},
                       device=os.environ.get("HEXW_BAKE_DEVICE", "CPU"), bakeStats=bake_stats,
                       aoDistanceM=AO_DIST,
                       culledReceiverFaces={"buried": culled, "inCrown": culled_crown, "of": faces_before},
                       skipped=skip_bake,
                       blend={"store": "local:assets-source/blend", "key": paths.blend_key(BLEND),
                              "sha256": bake.sha256_file(BLEND)})
