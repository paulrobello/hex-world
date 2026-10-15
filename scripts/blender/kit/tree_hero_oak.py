"""tree_hero_oak export stage: opens the live-authored high-poly .blend (R1: local, hashed) and runs
the shared low/bake/LOD/export pipeline headless. Usage: make kit-build PIECE=tree_hero_oak.

Three materials on one UV0 (owner: solid-dome canopy; P2 polish in `_oak`):
- Canopy: one closed low-poly R0 dome shell (`_oak.dome_sector`, SECTORS-fold, every sector on the
  same UV island set) packed below v = _tree.LEAF_V0 around the bark islands and baked
  selected-to-active from the leaf-covered smooth master dome `tree_hero_oak_canopy_high` (albedo
  through EMIT, normal, AO into ORM.R). Every clump is a copy of that shell with its own lumps
  (`_oak.place_shell`), so they share the bake. Slot `unique` + masks: masks.A = 1 on its islands
  (its AO is ORM.R), R snow capacity follows the canopy AO, G wetness.
- Bark: the tileable `bark` set mapped on UV0 (metres x BARK_UV_PER_M, `tileUvScale`); its AO is in
  masks.A, on the same UV0, never overlapping the canopy islands.
- Fringe leaves: alpha cards on the unique strip v >= LEAF_V0 (`_leaves`), masks.A = 1 there.

Built and baked at 1x; every LOD and the collider ship at SCALE (1.2x, owner 2026-10-14)."""
import math
import os
import pathlib
import sys

import bmesh
import bpy
import numpy as np

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
from mathutils import Matrix  # noqa: E402

from lib import bake, collider, export, geom, lod, paths, scene, uv  # noqa: E402
import _leaves  # noqa: E402
import _oak  # noqa: E402
import _tree  # noqa: E402

PIECE, ATLAS, MASK_SIZE = "tree_hero_oak", 2048, 2048
BARK_SET_PX, BARK_TILE_M = 1024, 2.0  # the `bark` tileable set (scripts/blender/kit/mat_bark.py)
# Owner 2026-10-14: the whole tree 20% bigger. Everything is built and baked at 1x (the canopy
# master, the crown layout and the leaf cards in the .blend are 1x), and every LOD and the collider
# are scaled by SCALE about the trunk base just before export.
SCALE = 1.2
# Atlas UV per 1x metre of bark: AO at MASK_SIZE / 54 / SCALE = 32 px/m. The bark column stays
# narrow (u < ~0.14), so the canopy islands keep >= 256 px/m on the 1.2x crown.
BARK_UV_PER_M = 1 / 54
TILE_UV_SCALE = SCALE / (BARK_UV_PER_M * BARK_TILE_M)  # manifest tree_hero_oak_bark.tileUvScale = 32.4
BLEND = pathlib.Path(os.environ.get("HEXW_BLEND") or paths.blend_path(PIECE))  # main-checkout store (O2)
CAGE, MAX_RAY, MARGIN, SAMPLES = 0.25, 0.5, 8, 32  # bark: cage clears the 6-sided receiver
SHELL_CAGE, SHELL_RAY = 0.45, 0.9  # canopy: clears leaves stacked up to ~0.25 m off the dome
# AO ray lengths: crevice scale. The world default (10 m) lets limbs 4 m up darken the whole trunk.
AO_DIST, SHELL_AO_DIST = 0.35, 0.3
CANOPY_MAT = f"{PIECE}_canopy"
ROOT_SIDES, ROOT_STEP, LIMB_SIDES, LIMB_STEP = 5, 1.3, 5, 1.6  # P2: heavier roots, visible limbs
GROUND_CULL_Z = -0.27  # below the meadow relief's floor (-0.25 m, meadow_base.relief)
BARK_FILLS = (0.6, 0.55, 0.5, 0.45, 0.4)  # packed share of the bark column tried, tightest first
SHELL_PACK_MARGIN = 0.008  # UV fraction between canopy and bark islands: 16 atlas and masks texels
# The sector unwraps as one island (minimum stretch, seams only on its two meridians); smart project
# at 1.0 rad split it into four islands that packed to 0.27 of the atlas, too little for 1.2x.
SHELL_UNWRAP = "MINIMUM_STRETCH"
# Level >= 1 branches (limbs, secondaries) as approved: the leaf and shell wind follow them.
LIMBS_SHA = "3b99db408e81160de6809d552d2b5623920967c2f2d57be6b64f3282b36a8c3a"
CANOPY_SNOW, WETNESS, NIGHT = 0.85, 0.5, 0.0  # masks R scale, G, B (trees do not glow at night)
# Canopy gaps (owner, 2026-10-14: the space between the leaves read shiny and pale): shell texels
# showing the dark dome under the baked leaves (`_oak.canopy_high`'s under tone, GAP_TONE sRGB; every
# leaf tone is >= 0.145 from it by max channel) take GAP_ALBEDO x their linear albedo and roughness
# GAP_ROUGH. The weight ramps 1 -> 0 over GAP_RAMP (max-channel distance), so a leaf-edge texel
# darkens only by its dome share and leaf texels stay byte-identical.
GAP_TONE = np.array([0x1F, 0x38, 0x12], np.float32) / 255
GAP_RAMP, GAP_ALBEDO, GAP_ROUGH = (0.01, 0.14), 0.5, 1.0
out = pathlib.Path(sys.argv[sys.argv.index("--") + 1])
skip_bake = os.environ.get("HEXW_SKIP_BAKE") == "1"

bpy.ops.wm.open_mainfile(filepath=str(BLEND))
scn = bpy.data.scenes[PIECE]
assert bpy.context.scene == scn, bpy.context.scene
bpy.context.preferences.filepaths.save_version = 0
HIGH_NAMES = (f"{PIECE}_leaves_high", f"{PIECE}_canopy_high")
# The showcase rig (lights, camera, ground, canopy instances) is not a bake input and never enters
# the export root.
for o in list(scn.objects):
    if o.name not in HIGH_NAMES:
        bpy.data.objects.remove(o, do_unlink=True)
bpy.context.view_layer.update()
leaves_high, canopy_high = (bpy.data.objects[n] for n in HIGH_NAMES)
canopy_high.hide_render = False  # hidden in the live showcase (instances show it), a bake source here
canopy_high.hide_set(False)
bark_mat, leaf_mat = bpy.data.materials[f"{PIECE}_bark"], bpy.data.materials[f"{PIECE}_leaves"]
canopy_mat = bpy.data.materials.get(CANOPY_MAT) or bpy.data.materials.new(CANOPY_MAT)
skel = _oak.skeleton()
crown = _tree.layout()
# The .blend's leaf cards and canopy master depend on the crown layout only. The roots changed in the
# trunk pass, so the bark high poly is built here from the current skeleton (the .blend's P3 bark
# high poly is not read), smooth: the tileable set carries the fissures, the bake only macro AO.
assert _tree.skeleton_sha([b for b in skel if b["level"] >= 1]) == LIMBS_SHA, "limbs changed"
bark_high = _tree.tube_mesh(f"{PIECE}_bark_high_gen", skel, {0: 40, 1: 28, 2: 16, 3: 10},
                            {0: 0.025, 1: 0.04, 2: 0.05, 3: 0.05})
bark_high.data.materials.append(bark_mat)


def _set_pins(mesh, value):
    pin = mesh.attributes.get(".pn.UVMap") or mesh.attributes.new(".pn.UVMap", "BOOLEAN", "CORNER")
    pin.data.foreach_set("value", [value] * len(mesh.loops))


def uv_area(obj):
    """Total UV0 area of obj's faces (atlas units squared)."""
    uvd, total = obj.data.uv_layers["UVMap"].data, 0.0
    for poly in obj.data.polygons:
        t = [uvd[li].uv for li in poly.loop_indices]
        for i in range(1, len(t) - 1):
            total += abs((t[i].x - t[0].x) * (t[i + 1].y - t[0].y) - (t[i + 1].x - t[0].x) * (t[i].y - t[0].y)) / 2
    return total


def pack_below_strip(obj, rotate, scale, margin, locked=(), margin_method="SCALED", right=1.0, check=True,
                     left=0.0):
    """Pack obj's UVMap islands into left < u < right, v < LEAF_V0, around locked placeholders over
    the leaf strip (and over u >= right and u <= left) and around the (already packed, unchanged)
    islands of the `locked` objects. Returns the islands' v max, or None (check=False) when they do
    not fit."""
    keep = []
    for lo in locked:
        a = np.empty(len(lo.data.loops) * 2)
        lo.data.uv_layers["UVMap"].data.foreach_get("uv", a)
        keep.append(a)
        _set_pins(lo.data, True)
    rects = ([(0.0, _tree.LEAF_V0, 1.0, 1.0)] + ([(right, 0.0, 1.0, _tree.LEAF_V0)] if right < 1.0 else [])
             + ([(0.0, 0.0, left, _tree.LEAF_V0)] if left > 0.0 else []))
    ph_mesh = bpy.data.meshes.new("leaf_strip_placeholder")
    ph_mesh.from_pydata([(x, y, -90) for _ in rects for x, y in ((0, 0), (1, 0), (1, 1), (0, 1))], [],
                        [[4 * i, 4 * i + 1, 4 * i + 2, 4 * i + 3] for i in range(len(rects))])
    ph = scene.link(bpy.data.objects.new("leaf_strip_placeholder", ph_mesh))
    ph_uv = ph_mesh.uv_layers.new(name="UVMap")
    corners = [c for u0, v0, u1, v1 in rects for c in ((u0, v0), (u1, v0), (u1, v1), (u0, v1))]
    for loop, co in zip(ph_mesh.loops, corners):
        ph_uv.data[loop.index].uv = co
    # Blender 5.x stores UV pins as the boolean corner attribute ".pn.<uv layer>".
    _set_pins(ph_mesh, True)
    scene.select_only([obj, ph, *locked], obj)
    bpy.ops.object.mode_set(mode="EDIT")
    bpy.ops.mesh.select_all(action="SELECT")
    bpy.ops.uv.select_all(action="SELECT")
    bpy.ops.uv.pack_islands(rotate=rotate, scale=scale, margin_method=margin_method, margin=margin,
                            pin=True, pin_method="LOCKED")
    bpy.ops.object.mode_set(mode="OBJECT")
    bpy.data.objects.remove(ph, do_unlink=True)
    for lo, a in zip(locked, keep):
        b = np.empty(len(lo.data.loops) * 2)
        lo.data.uv_layers["UVMap"].data.foreach_get("uv", b)
        assert np.array_equal(a, b), f"{lo.name}: locked UV islands moved in the pack"
        lo.data.attributes.remove(lo.data.attributes[".pn.UVMap"])
    a = np.empty(len(obj.data.loops) * 2)
    obj.data.uv_layers["UVMap"].data.foreach_get("uv", a)
    vmax = float(a[1::2].max())
    fits = (vmax < _tree.LEAF_V0 and a.min() >= 0.0 and float(a[0::2].max()) <= right and a.max() <= 1.0
            and float(a[0::2].min()) >= left)
    assert fits or not check, (obj.name, a.min(), vmax, a.max())
    return vmax if fits else None


# LOD0 bark receiver: the same skeleton at low side counts, closed tubes; faces buried in another
# branch or hidden inside a canopy dome are culled. (branches, level, ring sides, ring step m).
trunk = skel[:3]
roots = [b for b in skel[3:] if b["level"] == 0]
limbs = [b for b in skel if b["level"] == 1]
twigs = [b for b in skel if b["level"] == 2]
groups = ((trunk, 0, 6, 0.6), (roots, 0, ROOT_SIDES, ROOT_STEP), (limbs, 1, LIMB_SIDES, LIMB_STEP),
          (twigs, 2, 4, 1.5))
bark_sides = {b["id"]: n for bs, _, n, _ in groups for b in bs}
parts = [_tree.tube_mesh(f"bark_{i}", bs, {lv: n}, {lv: st}) for i, (bs, lv, n, st) in enumerate(groups)]
for part, (bs, _, n, st) in zip(parts, groups):
    _oak.metric_tube_uv(part, bs, n, st)
bark_low = geom.join(parts, "bark_low")
faces_before = len(bark_low.data.polygons)
culled = _tree.cull_buried(bark_low, skel, bark_sides)
culled_crown = _tree.cull_inside_crown(bark_low, crown)
culled_ground = _oak.cull_below(bark_low, GROUND_CULL_Z)
print(f"{PIECE}: culled {culled} buried, {culled_crown} in-crown and {culled_ground} underground "
      f"of {faces_before} bark faces")
bark_low.data.materials.append(bark_mat)
lod.triangulate(bark_low)
me = bark_low.data
bv = np.empty(len(me.loops) * 2)
me.uv_layers["UVMap"].data.foreach_get("uv", bv)
me.uv_layers["UVMap"].data.foreach_set("uv", bv * BARK_UV_PER_M)
# Unrotated and unscaled, so the grain stays along every branch and tileUvScale is exact.
# The bark packs into a left column (BARK_FILLS), so the canopy islands get one large, near-square
# free block beside it instead of the gaps between scattered bark islands.
bark_area = uv_area(bark_low)

# Canopy shell: one sector unwrapped (one island) and packed below the strip, then closed by rotation
# (the other sectors reuse its UVs, and the SECTORS-fold master bakes identically into every sector).
# The canopy slot samples masks at raw UV0, so its island packs beside the bark column, never on it.
shell = _oak.dome_sector("shell_base")
scene.select_only([shell])
shell.data.uv_layers.new(name="UVMap")
bpy.ops.object.mode_set(mode="EDIT")
bpy.ops.mesh.select_all(action="SELECT")
bpy.ops.uv.unwrap(method=SHELL_UNWRAP, margin=0.01)
bpy.ops.uv.pack_islands(rotate=True, margin=0.01)
bpy.ops.object.mode_set(mode="OBJECT")
shell_uv = np.empty(len(shell.data.loops) * 2)
shell.data.uv_layers["UVMap"].data.foreach_get("uv", shell_uv)
# The narrowest bark column (BARK_FILLS, tightest first) that the unscaled bark islands fit in and
# that the scaled shell island then fits beside, packed against a placeholder over the whole column
# (with the bark islands locked instead, the packer overran the strip with the one large island).
shell_vmax = None
for fill in BARK_FILLS:
    bark_right = min(1.0, bark_area / fill / _tree.LEAF_V0)
    bark_vmax = pack_below_strip(bark_low, rotate=False, scale=False, margin=0.003, right=bark_right, check=False)
    if bark_vmax is None:
        continue
    shell.data.uv_layers["UVMap"].data.foreach_set("uv", shell_uv)
    shell_vmax = pack_below_strip(shell, rotate=True, scale=True, margin=SHELL_PACK_MARGIN,
                                  margin_method="FRACTION", check=False, left=bark_right + SHELL_PACK_MARGIN)
    if shell_vmax is not None:
        break
assert shell_vmax is not None, f"bark islands (UV area {bark_area:.4f}) and the shell fit no column"
print(f"{PIECE}: bark UV area {bark_area:.4f}, column u < {bark_right:.4f} (fill {fill}), shell UV area "
      f"{uv_area(shell):.4f}")
_tree.bark_wind_uv(bark_low)  # TEXCOORD_1 after the atlas pack, so UVMap stays first and active
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
else:
    ao = np.ones((MASK_SIZE, MASK_SIZE), np.float32)


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


def dilate(mask, n):
    """Grow a bool texel mask by n texels (4-neighbour steps)."""
    m = mask.copy()
    for _ in range(n):
        g = m.copy()
        g[1:] |= m[:-1]
        g[:-1] |= m[1:]
        g[:, 1:] |= m[:, :-1]
        g[:, :-1] |= m[:, 1:]
        m = g
    return m


def stats(a):
    return {"texels": int(a.size), "min": float(a.min()), "mean": float(a.mean()),
            "p01": float(np.percentile(a, 1)), "p05": float(np.percentile(a, 5))}


# masks (raw UV0, MASK_SIZE): bark texels carry the bark AO bake (its gutter stays inside a 6-texel
# dilation, short of the canopy islands SHELL_PACK_MARGIN away); canopy texels carry A = 1 (their AO
# is ORM.R) and snow capacity R from the canopy AO, so snow settles on the leaves, not in crevices.
bark_isl = island_texels(bark_low, MASK_SIZE)
canopy_isl = island_texels(shell, MASK_SIZE)
canopy_isl_atlas = island_texels(shell, ATLAS)
bark_zone = dilate(bark_isl, 6)
canopy_zone = dilate(canopy_isl, 6)
overlap = int((bark_isl & canopy_zone).sum())
assert overlap == 0, f"bark islands within 6 masks texels of the canopy islands: {overlap}"
canopy_ao_m = orm[..., 0].reshape(MASK_SIZE, ATLAS // MASK_SIZE, MASK_SIZE, ATLAS // MASK_SIZE).mean(axis=(1, 3))
mk = np.empty((MASK_SIZE, MASK_SIZE, 4), np.float32)
mk[...] = (1.0, WETNESS, NIGHT, 1.0)
mk[..., 3] = np.where(bark_zone, ao, 1.0)
mk[..., 0] = np.where(canopy_zone, CANOPY_SNOW * np.clip((canopy_ao_m - 0.3) / 0.5, 0.0, 1.0), 1.0)
bake._write(MASK_SIZE, mk.reshape(-1, 4), masks_path)
if not skip_bake:
    bake_stats["barkAo"] = stats(ao[bark_isl])
    # Texels inside another branch's low tube are never seen (partly buried receiver faces).
    _, bark_hidden = _oak.hidden_texels(bark_low, skel, bark_sides,
                                        MASK_SIZE)
    bake_stats["barkAoVisible"] = stats(ao[bark_isl & ~bark_hidden])
    bake_stats["barkAoVisible"]["hiddenTexels"] = int((bark_isl & bark_hidden).sum())
    bake_stats["canopyAo"] = stats(orm[..., 0][canopy_isl_atlas])
    bake_stats["canopyMasks"] = {"aMin": float(mk[..., 3][canopy_isl].min()),
                                 "snowMean": float(mk[..., 0][canopy_isl].mean())}


def srgb_to_lin(c):
    return np.where(c <= 0.04045, c / 12.92, ((c + 0.055) / 1.055) ** 2.4)


def lin_to_srgb(c):
    return np.where(c <= 0.0031308, c * 12.92, 1.055 * np.maximum(c, 0.0031308) ** (1 / 2.4) - 0.055)


# Canopy gaps: the shell islands plus their bake gutter, so bilinear and mip taps at a rim match.
gap_zone = dilate(canopy_isl_atlas, MARGIN)
gap_w = np.where(gap_zone, np.clip((GAP_RAMP[1] - np.abs(albedo[..., :3] - GAP_TONE).max(-1))
                                   / (GAP_RAMP[1] - GAP_RAMP[0]), 0.0, 1.0), 0.0).astype(np.float32)
gap_lin = srgb_to_lin(albedo[..., :3]) - (gap_w * (1.0 - GAP_ALBEDO))[..., None] * srgb_to_lin(GAP_TONE)
albedo[..., :3] = np.where(gap_w[..., None] > 0, lin_to_srgb(np.clip(gap_lin, 0.0, 1.0)), albedo[..., :3])
orm[..., 1] += gap_w * (GAP_ROUGH - orm[..., 1])
if not skip_bake:
    gap_texels = canopy_isl_atlas & (gap_w > 0.5)
    bake_stats["canopyGaps"] = {"share": float(gap_texels.sum() / canopy_isl_atlas.sum()),
                                "albedoScaleLinear": GAP_ALBEDO, "roughness": GAP_ROUGH,
                                "albedoSrgbMean": [float(x) for x in albedo[..., :3][gap_texels].mean(0)]}
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

# Canopy LOD0: one lumped copy of the shell per clump (shared bake), with its wind UV; faces hidden
# inside a neighbouring clump are culled.
shells = []
for cl in crown:
    o = _oak.place_shell(shell, cl, f"shell_{cl['id']:02d}")
    _tree.shell_wind_uv(o, cl, skel)
    shells.append(o)
# Closed, uncut per-clump sources for the LOD1/LOD2 shells: collapse-decimating a closed dome stays
# round, while decimating the culled (open-edged) LOD0 canopy left spikes at its cut rims.
shell_srcs = []
for o in shells:
    c_ = o.copy()
    c_.data = o.data.copy()
    c_.name = c_.data.name = f"{o.name}_src"
    shell_srcs.append(scene.link(c_))
canopy_low = geom.join(shells, "canopy_low")
bpy.data.objects.remove(shell, do_unlink=True)
shell_faces = len(canopy_low.data.polygons)
culled_shell = _oak.cull_shell_hidden(canopy_low, crown)
print(f"{PIECE}: culled {culled_shell} hidden of {shell_faces} shell faces")
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


def lod_canopy(per_clump, name):
    """Each closed clump shell decimated to per_clump tris and joined. No hidden-face cull here: a
    coarse neighbour sits well inside its smooth surface, so culling against that surface would open
    see-through gaps at the dome junctions."""
    parts_ = []
    for src in shell_srcs:
        o = copy_of(src, f"{src.name}_{name}")
        lod.decimate_to(o, per_clump)
        parts_.append(o)
    return geom.join(parts_, name)



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



def at_scale(d):
    """A 1x texel-density measurement on the SCALE-d export (uniform scale: px/m divides by it)."""
    return {k: v / SCALE for k, v in d.items()}


# Texel density per material (owner ruling): bark against its tileable set through tileUvScale,
# canopy and fringe against the unique atlas. Each median must reach 256 px/m on its own, measured
# at the exported scale.
texel = {f"{PIECE}_bark": at_scale(uv.measure_texel_density(bark_low, TILE_UV_SCALE * BARK_SET_PX)),
         CANOPY_MAT: at_scale(uv.measure_texel_density(canopy_low, ATLAS)),
         f"{PIECE}_leaves": at_scale(uv.measure_texel_density(leaves_low, ATLAS))}
ao_density = at_scale(uv.measure_texel_density(bark_low, MASK_SIZE))


def uv_stretch(obj, material, z_max):
    """Bark UV anisotropy per triangle (largest over smallest singular value of the 3D -> UV map,
    1 = undistorted) on `material` faces whose lowest vertex is below z_max: p50, p95 and max."""
    me, vals = obj.data, []
    uvd = me.uv_layers["UVMap"].data
    idx = {i for i, m in enumerate(me.materials) if m and m.name == material}
    for poly in me.polygons:
        if poly.material_index not in idx:
            continue
        li = list(poly.loop_indices)
        q = [np.array(me.vertices[me.loops[i].vertex_index].co[:]) for i in li]
        if min(v[2] for v in q) >= z_max:
            continue
        t = [np.array(uvd[i].uv[:]) for i in li]
        e1, e2 = q[1] - q[0], q[2] - q[0]
        n = np.cross(e1, e2)
        if np.linalg.norm(n) < 1e-10:
            continue
        ax = e1 / np.linalg.norm(e1)
        ay = np.cross(n / np.linalg.norm(n), ax)
        x = np.array([[e1 @ ax, e2 @ ax], [e1 @ ay, e2 @ ay]])
        u = np.column_stack([t[1] - t[0], t[2] - t[0]])
        if abs(np.linalg.det(x)) < 1e-12 or abs(np.linalg.det(u)) < 1e-14:
            continue
        sv = np.linalg.svd(u @ np.linalg.inv(x), compute_uv=False)
        vals.append(float(sv[0] / sv[1]))
    vals = np.array(vals)
    return {"faces": int(vals.size), "p50": float(np.percentile(vals, 50)), "p95": float(np.percentile(vals, 95)),
            "max": float(vals.max())}


# LODs: canopy shells decimate; fringe keeps card subsets; bark takes the rest of each budget.
lods = []
parts0 = [geom.tri_count(o) for o in (bark_low, canopy_low, leaves_low)]
t0 = sum(parts0)
for i, (b, lv) in enumerate(((bark_low, leaves_low), (bark1, leaves1), (bark2, leaves2))):
    c = canopy_low
    if i:
        budget = int(t0 * (0.5 if i == 1 else 0.15)) - geom.tri_count(lv) - 4
        c = lod_canopy(int(budget * (0.55 if i == 1 else 0.6)) // len(crown), f"canopy_l{i}")
        lod.decimate_to(b, budget - geom.tri_count(c))
    o = geom.join([b, c, lv], f"{PIECE}_lod{i}")
    _tree.clamp_wind_uv(o)
    _oak.shell_normals(o, CANOPY_MAT, crown)  # smooth domes at every LOD, then bent by the next call
    _tree.set_canopy_normals(o, leaf_mat.name, CANOPY_MAT)
    joined = _oak.junction_normals(o, skel, bark_mat.name)
    print(f"{PIECE}: lod{i} junction/ground normals on {joined} bark vertices")
    lods.append(o)
for src in shell_srcs:
    bpy.data.objects.remove(src, do_unlink=True)
tris = [geom.tri_count(o) for o in lods]
print(f"{PIECE}: LOD tris {tris}, LOD0 bark/canopy/fringe {parts0}")
assert tris[0] <= 3000 and tris[1] <= 0.5 * tris[0] and tris[2] <= 0.15 * tris[0], tris
print(f"{PIECE}: tris per LOD {tris} (LOD0 bark/canopy/fringe {parts0}), texel {texel}, "
      f"bark AO texels {ao_density}, bark v max {bark_vmax:.4f}, shell v max {shell_vmax:.4f}")
for name, t in texel.items():
    assert t["median"] >= 256, (name, t)
stretch = uv_stretch(lods[0], bark_mat.name, 2.6)
print(f"{PIECE}: bark UV stretch below 2.6 m (1x) {stretch}")
# 1.2x: scale every LOD and the collider about the trunk base, keeping each corner normal.
for o in (*lods, col):
    keep = [tuple(c.vector) for c in o.data.corner_normals]
    o.data.transform(Matrix.Scale(SCALE, 4))
    if o is not col:
        o.data.normals_split_custom_set(keep)
    o.data.update()
    after = [tuple(c.vector) for c in o.data.corner_normals]
    assert max(max(abs(a - b) for a, b in zip(u, v)) for u, v in zip(keep, after)) < 1e-4, o.name
sig = bake.receiver_signature([lods[0]])

root = scene.empty(PIECE)
for o in lods:
    o.parent = root
col.parent = root
export.export_glb(root, str(out / f"{PIECE}.glb"))
export.write_export_report(str(out / "export-report.json"), root, PIECE, __file__, budget_class="hero")
bake.write_bake_report(str(out / "bake-report.json"), receiverSignature=sig, samples=SAMPLES, marginPx=MARGIN,
                       cageExtrusionM=CAGE, maxRayDistanceM=MAX_RAY, texelDensityPxPerM=texel,
                       scale=SCALE, barkUvStretch=stretch,
                       canopyShell={"cageExtrusionM": SHELL_CAGE, "maxRayDistanceM": SHELL_RAY,
                                    "aoDistanceM": SHELL_AO_DIST, "sectors": _tree.SECTORS,
                                    "clumps": len(crown), "columnsPerSector": _oak.SHELL_COLUMNS,
                                    "ringLatitudes": list(_oak.SHELL_PHIS),
                                    "culledHiddenFaces": {"culled": culled_shell, "of": shell_faces}},
                       barkTile={"set": "bark", "tileUvScale": TILE_UV_SCALE, "uvPerM": BARK_UV_PER_M,
                                 "aoTexelDensityPxPerM": ao_density},
                       channels={"albedo": {"size": ATLAS, "colorspace": "srgb"},
                                 "normal": {"size": ATLAS, "colorspace": "linear"},
                                 "orm": {"size": ATLAS, "colorspace": "linear"},
                                 "masks": {"size": MASK_SIZE, "colorspace": "linear"}},
                       device=os.environ.get("HEXW_BAKE_DEVICE", "CPU"), bakeStats=bake_stats,
                       aoDistanceM=AO_DIST,
                       culledReceiverFaces={"buried": culled, "inCrown": culled_crown, "underground": culled_ground,
                                            "of": faces_before},
                       skipped=skip_bake,
                       blend={"store": "local:assets-source/blend", "key": paths.blend_key(BLEND),
                              "sha256": bake.sha256_file(BLEND)})
