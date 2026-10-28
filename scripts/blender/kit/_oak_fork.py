"""Hero oak, forked variant: the builder behind tree_hero_oak_b.py (PRD §6 Variety, the oak tile's hero
slot). Refs: assets-source/refs/tree_hero_oak (ref-1, owner-approved) and assets-source/refs/forest.

The hero oak's settled look (bark set on UV0 at the hero's tileUvScale, the leaf-covered master dome
baked into SECTORS-fold clump shells, lobed fringe cards, palette, canopy gaps, wind data, AO) with a
different form: the three twisted strands lean toward +x and fork at about 3.3 m into two leaders,
a tall east lobe and a lower west lobe, and one heavy low limb runs out nearly level to the south
under a low shelf of domes. The crown is a crescent in plan, open to the north-west (where the fork
shows through), against the hero's round, tiered dome.

The approved helpers (`_tree`, `_oak`, `_leaves`) run unchanged: their functions read their module
globals at call time, so `configure` points the per-piece seeds at this piece (clump lumps, branch,
card and clump wind phases) and asserts the invariants the copied steps rely on. The canopy master and
the leaf tiles keep the hero's seed, so every dome and card shows the hero's leaves. The small pipeline
helpers come from `_oak_scatter` (imported, never configured here); only the monolithic steps of
`tree_hero_oak.py` (pack, bakes, masks, gaps, LOD joins, scale) are copied. No live .blend: every high
poly is built here, so the route is scripted. Built and baked at 1x; every LOD and the collider ship at
SCALE (the hero's 1.2x).

Leaf roughness is matte (owner A35 O1, 2026-10-27): the shell's 0.62 and the leaf strip's
0.50-0.66 go through `_verge_bush.matte` (texels under 0.7 map linearly onto 0.76-0.86) before
the canopy gaps blend toward 1.0, as the verge bushes and woodland shrubs were matted."""
from __future__ import annotations

import math
import os
import pathlib
import random

import bmesh
import bpy
import numpy as np
from mathutils import Matrix, Vector

from lib import bake, budgets, collider, export, geom, lod, materials, scene, uv
import _leaves
import _oak
import _tree
import _verge_bush
from _oak_scatter import (card_subset, copy_of, dilate, lin_to_srgb, pack_below_strip, radial_max, srgb_to_lin,
                          stats, uv_area)
from _shared import island_texels

HERO = "tree_hero_oak"  # the canopy master's and leaf tiles' seed: the hero's leaves on every dome and card
ATLAS, MASK_SIZE, SCALE = 2048, 2048, 1.2  # as the hero (KTX2 class `hero`)
BARK_SET_PX, BARK_TILE_M = 1024, 2.0
BARK_UV_PER_M = 1 / 54  # the hero's: bark AO 32 px/m at 1.2x, tileUvScale 32.4
TILE_UV_SCALE = SCALE / (BARK_UV_PER_M * BARK_TILE_M)
CAGE, MAX_RAY, MARGIN, SAMPLES = 0.25, 0.5, 8, 32
SHELL_CAGE, SHELL_RAY = 0.45, 0.9
AO_DIST, SHELL_AO_DIST = 0.35, 0.3
ROOT_SIDES, ROOT_STEP, LIMB_SIDES, LIMB_STEP = 5, 1.3, 5, 1.6
GROUND_CULL_Z = -0.27
BARK_FILLS = (0.6, 0.55, 0.5, 0.45, 0.4, 0.35, 0.3)
SHELL_PACK_MARGIN = 0.008
SHELL_UNWRAP = "MINIMUM_STRETCH"
CANOPY_SNOW, WETNESS, NIGHT = 0.85, 0.5, 0.0
GAP_TONE = np.array([0x1F, 0x38, 0x12], np.float32) / 255
GAP_RAMP, GAP_ALBEDO, GAP_ROUGH = (0.01, 0.14), 0.5, 1.0
HERO_SHELL_RINGS = ((-67.5, 3), (-45, 4), (-22.5, 7), (0, 7), (22.5, 7), (45, 5), (67.5, 3))


def configure(piece: str) -> None:
    """Per-piece seeds on the hero helpers, and the invariants the copied steps rely on."""
    _tree.SEED = piece
    _oak.SEED = piece
    assert _tree.SECTORS == _oak.SECTORS == 3, (_tree.SECTORS, _oak.SECTORS)
    assert _oak.SHELL_RINGS == HERO_SHELL_RINGS, _oak.SHELL_RINGS
    assert (_oak.R0, _oak.DOME_WIDE, _oak.DOME_TOP, _oak.DOME_BASE) == (2.0, 1.15, 0.9, 0.4)
    assert _tree.LEAF_V0 == 0.75 and _tree.LEAF_TILES == _leaves.TILES == 4
    assert round((1 - _tree.LEAF_V0) * ATLAS) == ATLAS // _leaves.TILES


def layout(piece: str, clumps: list[tuple]) -> list[dict]:
    """The crown as `_tree.layout` returns it ({id, tier, centre, radius, rot, stretch}) from explicit
    (x, y, z, radius, feed) specs; rot and stretch are seeded. feed names the branch that runs into
    the clump: leader_a / leader_b (tier 2), limb / heavy (tier 0), sec (tier 1)."""
    rng = random.Random(piece + ":layout")
    tier = {"leader_a": 2, "leader_b": 2, "limb": 0, "heavy": 0, "sec": 1}
    return [{"id": i, "tier": tier[feed], "feed": feed, "centre": Vector((x, y, z)), "radius": r,
             "rot": rng.uniform(0, 2 * math.pi), "stretch": (rng.uniform(0.92, 1.08), rng.uniform(0.92, 1.08))}
            for i, (x, y, z, r, feed) in enumerate(clumps)]


def _smoothstep(x: float) -> float:
    x = min(1.0, max(0.0, x))
    return x * x * (3 - 2 * x)


def skeleton(crown: list[dict], seed: str, s: dict):
    """Branches as `_tree.skeleton` returns them ({id, level, points, radii, sway}), and the lean
    function. Level 0: the hero's three twisted strands plus a lean growing as (z / 3.7)^2 (none at
    the base, so the trunk base stays on the pivot), and the hero's eight buttress roots (its root
    loop, on the leaning strands). Level 1: two fork leaders from the strand tops into the lobe tops,
    limbs, and the heavy low limb. Level 2: secondaries from the nearest level-1 branch."""
    rng = random.Random(seed)
    lean_v = Vector((*s["lean"], 0.0))

    def lean(z: float) -> Vector:
        return lean_v * (max(z, 0.0) / 3.7) ** 2

    def strand(i: int, t: float) -> Vector:
        p = _oak._strand_point(i, t)
        return p + lean(p.z)

    out: list[dict] = []
    for i in range(3):
        out.append({"level": 0, "points": [strand(i, k / 40) for k in range(41)],
                    "radii": [_oak._strand_radius(k / 40) for k in range(41)], "sway": (0.0, _tree.SWAY_TRUNK)})
    # `_oak.skeleton`'s roots (trunk pass), on the leaning strands.
    for i in range(8):
        a = 2 * math.pi * (i + rng.uniform(-0.25, 0.25)) / 8
        reach = rng.uniform(2.4, 3.2) * 1.15
        z0 = _oak.ROOT_Z0[0] + (_oak.ROOT_Z0[1] - _oak.ROOT_Z0[0]) * _tree.hash01(f"root_z0:{i}")
        ts = (z0 + 0.3) / 4.0
        sp = min((strand(j, ts) for j in range(3)),
                 key=lambda p: abs(math.remainder(math.atan2(p.y, p.x) - a, 2 * math.pi)))
        pts, radii = [], []
        for k in range(21):
            t = k / 20
            w = 0.18 * math.sin(3 * t + i)
            e = Vector((reach * math.cos(a + w), reach * math.sin(a + w)))
            h = sp.xy + (e - sp.xy) * t ** _oak.ROOT_OUT_POW
            pts.append(Vector((h.x, h.y, z0 * (1 - t) ** 2.0 + _oak.ROOT_TIP_Z * t)))
            rise = _oak.ROOT_R_START + (_oak.ROOT_R_PEAK - _oak.ROOT_R_START) * _smoothstep(t / _oak.ROOT_T_PEAK)
            radii.append(min(rise, 0.13 + (_oak.ROOT_R_PEAK + 0.06 - 0.13) * (1 - t) ** 0.6))
        out.append({"level": 0, "points": _tree._smooth(pts), "radii": radii, "sway": (0.0, 0.0)})
    feeders = []  # (points, sway at start, sway at end) of the level-1 branches secondaries may leave
    # Fork leaders: from inside the strand bundle near its top, out to each lobe's top clump.
    for key in ("leader_a", "leader_b"):
        for cl in [c for c in crown if c["feed"] == key]:
            c = cl["centre"]
            d = Vector((c.x, c.y, 0.0)).normalized()
            z0 = s["fork_z"]
            start = Vector((0.0, 0.0, z0)) + lean(z0) + d * 0.12
            r0, r1 = s[key + "_r"]
            pts = _tree._arc(rng, start, c - Vector((0, 0, 0.4)), 0.25, 0.1)
            feeders.append(pts)
            out.append({"level": 1, "points": pts, "radii": _tree._lerp_radius(r0, r1, len(pts), 0.9),
                        "sway": (_tree.SWAY_TRUNK, _tree.SWAY_LIMB)})
    # Limbs (the hero's: leave the trunk low, run out nearly level, rise into the clump) and the heavy
    # limb (lower, thicker, flatter for longer).
    for cl in [c for c in crown if c["feed"] in ("limb", "heavy")]:
        c = cl["centre"]
        heavy = cl["feed"] == "heavy"
        a = math.atan2(c.y - lean(2.5).y, c.x - lean(2.5).x)
        z0 = s["heavy_z"] if heavy else rng.uniform(*s["limb_z"])
        start = Vector((0.3 * math.cos(a), 0.3 * math.sin(a), z0)) + lean(z0)
        end = c - Vector((0, 0, 0.5))
        pts = _tree._arc(rng, start, end, 0.1 if heavy else 0.15, 0.14)
        ease = s["heavy_ease"] if heavy else 1.8
        for k, p in enumerate(pts):
            t = k / (len(pts) - 1)
            p.z = start.z + (end.z - start.z) * t ** ease + (p.z - (start.z + (end.z - start.z) * t))
        r0, r1 = s["heavy_r"] if heavy else s["limb_r"]
        feeders.append(pts)
        out.append({"level": 1, "points": pts, "radii": _tree._lerp_radius(r0, r1, len(pts), 0.8),
                    "sway": (_tree.SWAY_TRUNK, _tree.SWAY_LIMB)})
    for cl in [c for c in crown if c["feed"] == "sec"]:
        c = cl["centre"]
        best = min(feeders, key=lambda pts: (pts[len(pts) // 2] - c).length)
        t = 0.45
        base = best[int(t * (len(best) - 1))]
        s2 = _tree.SWAY_TRUNK + (_tree.SWAY_LIMB - _tree.SWAY_TRUNK) * t
        out.append({"level": 2, "points": _tree._arc(rng, base, c - Vector((0, 0, 0.3)), 0.4, 0.12, 14),
                    "radii": _tree._lerp_radius(*s["sec_r"], 15), "sway": (s2, _tree.SWAY_TWIG)})
    for i, b in enumerate(out):
        b["id"] = i
    return out, lean


def build(piece: str, out: pathlib.Path, generator: str, *, clumps: list[tuple], shape: dict) -> None:
    """The hero-oak pipeline for `piece` from a scripted high poly: skeleton and crown, LOD0 bark,
    canopy shell, fringe, atlas pack, bakes, maps, LODs, collider, scale, export and reports."""
    configure(piece)
    skip_bake = os.environ.get("HEXW_SKIP_BAKE") == "1"
    scene.reset_scene(piece)
    scn = bpy.context.scene
    scn.world = scn.world or bpy.data.worlds.new(piece)
    canopy_mat_name = f"{piece}_canopy"
    bark_mat = materials.pbr_material(f"{piece}_bark", materials.srgb("#6b5444"), 0.85)
    leaf_mat = materials.pbr_material(f"{piece}_leaves", materials.srgb("#4f7a2c"), 0.6)
    canopy_mat = materials.pbr_material(canopy_mat_name, materials.srgb("#3f6a24"), 0.62)
    crown = layout(piece, clumps)
    skel, lean = skeleton(crown, piece, shape)

    # LOD0 bark receiver (the hero's groups, sides and steps).
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
    print(f"{piece}: culled {culled} buried, {culled_crown} in-crown and {culled_ground} underground "
          f"of {faces_before} bark faces")
    bark_low.data.materials.append(bark_mat)
    lod.triangulate(bark_low)
    me = bark_low.data
    bv = np.empty(len(me.loops) * 2)
    me.uv_layers["UVMap"].data.foreach_get("uv", bv)
    me.uv_layers["UVMap"].data.foreach_set("uv", bv * BARK_UV_PER_M)
    bark_area = uv_area(bark_low)

    # Canopy shell: one sector, one island, packed beside the bark column (the hero's steps).
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
    shell_vmax = bark_vmax = None
    for fill in BARK_FILLS:
        bark_right = min(1.0, bark_area / fill / _tree.LEAF_V0)
        bark_vmax = pack_below_strip(bark_low, rotate=False, scale=False, margin=0.003, right=bark_right)
        if bark_vmax is None:
            continue
        shell.data.uv_layers["UVMap"].data.foreach_set("uv", shell_uv)
        shell_vmax = pack_below_strip(shell, rotate=True, scale=True, margin=SHELL_PACK_MARGIN,
                                      margin_method="FRACTION", left=bark_right + SHELL_PACK_MARGIN)
        if shell_vmax is not None:
            break
    assert shell_vmax is not None, f"bark islands (UV area {bark_area:.4f}) and the shell fit no column"
    shell_area = uv_area(shell)
    print(f"{piece}: bark UV area {bark_area:.4f}, column u < {bark_right:.4f} (fill {fill}), shell UV area "
          f"{shell_area:.4f}")
    _tree.bark_wind_uv(bark_low)
    _tree.replicate_sectors(shell)
    shell.data.materials.append(canopy_mat)
    lod.triangulate(shell)
    _tree.shell_smooth_normals(shell)

    # Fringe: the hero's lobed cards on these clumps (own seed, the hero's count per clump).
    leaves_low = _oak.leaf_cards("leaves_low", crown, seed=piece, per_clump=_tree.FRINGE_PER_CLUMP)
    leaves_low.data.materials.append(leaf_mat)
    _tree.leaf_wind_uv(leaves_low, skel)

    # Maps: the hero's master dome -> shell (albedo through EMIT, normal, AO); bark AO -> masks.A.
    strip_px = ATLAS - int(round(_tree.LEAF_V0 * ATLAS))
    leaf_alb, leaf_nrm, leaf_rough = _leaves.strip(strip_px, HERO)
    assert leaf_alb.shape[1] == ATLAS, leaf_alb.shape
    albedo = np.empty((ATLAS, ATLAS, 4), np.float32)
    albedo[:] = (*leaf_alb[..., :3].reshape(-1, 3).mean(0), 1.0)
    normal = np.empty((ATLAS, ATLAS, 4), np.float32)
    normal[:] = (0.5, 0.5, 1.0, 1.0)
    orm = np.empty((ATLAS, ATLAS, 4), np.float32)
    orm[:] = (1.0, 0.62, 0.0, 1.0)
    bake_stats = {}
    if not skip_bake:
        canopy_high = _oak.canopy_high(f"{piece}_canopy_high", seed=HERO, per_sector=480)
        canopy_high.data.materials.append(_tree.canopy_high_material(f"{piece}_canopy_high"))
        bark_high = _tree.tube_mesh(f"{piece}_bark_high", skel, {0: 40, 1: 28, 2: 16, 3: 10},
                                    {0: 0.025, 1: 0.04, 2: 0.05, 3: 0.05})
        bark_high.data.materials.append(bark_mat)
        bake.setup_cycles(samples=SAMPLES)
        kw = dict(cage_extrusion=SHELL_CAGE, max_ray=SHELL_RAY, margin=MARGIN)
        imgs = {k: bake.new_image(f"{piece}.shell_{k}", ATLAS, non_color=k != "albedo")
                for k in ("albedo", "normal", "ao")}
        bake.bake_emit_input([canopy_high], shell, "Base Color", imgs["albedo"], **kw)
        bake.bake_to(shell, "NORMAL", imgs["normal"], highs=[canopy_high], **kw)
        scn.world.light_settings.distance = SHELL_AO_DIST
        bake.bake_to(shell, "AO", imgs["ao"], highs=[canopy_high], **kw)
        px = {}
        for k, im in imgs.items():
            p = bake.save_png(im, str(out / f"{piece}.shell_{k}.tmp.png"))
            px[k] = bake.load_png(p)[1].reshape(ATLAS, ATLAS, 4)
            os.remove(p)
        albedo[:] = px["albedo"]
        albedo[..., 3] = 1.0
        normal[:] = px["normal"]
        normal[..., 3] = 1.0
        orm[..., 0] = px["ao"][..., 0]
        scn.world.light_settings.distance = AO_DIST
        ao_img = bake.new_image(f"{piece}.ao", MASK_SIZE, non_color=True)
        bake.bake_to(bark_low, "AO", ao_img, highs=[bark_high], cage_extrusion=CAGE, max_ray=MAX_RAY, margin=MARGIN)
        ao_tmp = bake.save_png(ao_img, str(out / f"{piece}.ao.tmp.png"))
        ao = bake.load_png(ao_tmp)[1][:, 0].reshape(MASK_SIZE, MASK_SIZE).copy()
        os.remove(ao_tmp)
        bpy.data.objects.remove(canopy_high, do_unlink=True)
        bpy.data.objects.remove(bark_high, do_unlink=True)
        bpy.context.view_layer.update()
    else:
        ao = np.ones((MASK_SIZE, MASK_SIZE), np.float32)

    bark_isl = island_texels(bark_low, MASK_SIZE)
    canopy_isl = island_texels(shell, MASK_SIZE)
    canopy_isl_atlas = island_texels(shell, ATLAS)
    bark_zone = dilate(bark_isl, 6)
    canopy_zone = dilate(canopy_isl, 6)
    overlap = int((bark_isl & canopy_zone).sum())
    assert overlap == 0, f"bark islands within 6 masks texels of the canopy islands: {overlap}"
    f_ = ATLAS // MASK_SIZE
    canopy_ao_m = orm[..., 0].reshape(MASK_SIZE, f_, MASK_SIZE, f_).mean(axis=(1, 3))
    mk = np.empty((MASK_SIZE, MASK_SIZE, 4), np.float32)
    mk[...] = (1.0, WETNESS, NIGHT, 1.0)
    mk[..., 3] = np.where(bark_zone, ao, 1.0)
    mk[..., 0] = np.where(canopy_zone, CANOPY_SNOW * np.clip((canopy_ao_m - 0.3) / 0.5, 0.0, 1.0), 1.0)
    bake.write_png(MASK_SIZE, mk.reshape(-1, 4), str(out / f"{piece}.masks.png"))
    if not skip_bake:
        bake_stats["barkAo"] = stats(ao[bark_isl])
        _, hidden = _oak.hidden_texels(bark_low, skel, bark_sides, MASK_SIZE)
        bake_stats["barkAoVisible"] = stats(ao[bark_isl & ~hidden])
        bake_stats["barkAoVisible"]["hiddenTexels"] = int((bark_isl & hidden).sum())
        bake_stats["canopyAo"] = stats(orm[..., 0][canopy_isl_atlas])
        bake_stats["canopyMasks"] = {"aMin": float(mk[..., 3][canopy_isl].min()),
                                     "snowMean": float(mk[..., 0][canopy_isl].mean())}

    gap_zone = dilate(canopy_isl_atlas, MARGIN)
    gap_w = np.where(gap_zone, np.clip((GAP_RAMP[1] - np.abs(albedo[..., :3] - GAP_TONE).max(-1))
                                       / (GAP_RAMP[1] - GAP_RAMP[0]), 0.0, 1.0), 0.0).astype(np.float32)
    gap_lin = srgb_to_lin(albedo[..., :3]) - (gap_w * (1.0 - GAP_ALBEDO))[..., None] * srgb_to_lin(GAP_TONE)
    albedo[..., :3] = np.where(gap_w[..., None] > 0, lin_to_srgb(np.clip(gap_lin, 0.0, 1.0)), albedo[..., :3])
    orm[..., 1] = _verge_bush.matte(orm[..., 1])
    orm[..., 1] += gap_w * (GAP_ROUGH - orm[..., 1])
    if not skip_bake:
        gap_texels = canopy_isl_atlas & (gap_w > 0.5)
        bake_stats["canopyGaps"] = {"share": float(gap_texels.sum() / canopy_isl_atlas.sum()),
                                    "albedoScaleLinear": GAP_ALBEDO, "roughness": GAP_ROUGH,
                                    "albedoSrgbMean": [float(x) for x in albedo[..., :3][gap_texels].mean(0)]}
    albedo[-strip_px:] = leaf_alb
    normal[-strip_px:, :, :3] = leaf_nrm
    orm[-strip_px:, :, 0] = 1.0
    orm[-strip_px:, :, 1] = _verge_bush.matte(leaf_rough)
    bake.write_png(ATLAS, albedo.reshape(-1, 4), str(out / f"{piece}.albedo.png"), non_color=False)
    bake.write_png(ATLAS, normal.reshape(-1, 4), str(out / f"{piece}.normal.png"))
    bake.write_png(ATLAS, orm.reshape(-1, 4), str(out / f"{piece}.orm.png"))
    cov, m = [], leaf_alb[..., 3]
    while min(m.shape) >= 1:
        cov.append(round(float((m > 0.5).mean()), 4))
        if min(m.shape) < 2:
            break
        h2, w2 = m.shape[0] // 2 * 2, m.shape[1] // 2 * 2
        m = m[:h2, :w2].reshape(h2 // 2, 2, w2 // 2, 2).mean(axis=(1, 3))
    bake_stats["leafAlphaCoverageByMip"] = cov

    # Canopy LOD0: one lumped copy of the shell per clump; closed copies feed the LOD1/LOD2 shells.
    shells = []
    for cl in crown:
        o = _oak.place_shell(shell, cl, f"shell_{cl['id']:02d}")
        _tree.shell_wind_uv(o, cl, skel)
        shells.append(o)
    shell_srcs = [copy_of(o, f"{o.name}_src") for o in shells]
    canopy_low = geom.join(shells, "canopy_low")
    bpy.data.objects.remove(shell, do_unlink=True)
    bpy.context.view_layer.update()
    shell_faces = len(canopy_low.data.polygons)
    culled_shell = _oak.cull_shell_hidden(canopy_low, crown)
    print(f"{piece}: culled {culled_shell} hidden of {shell_faces} shell faces; bake stats {bake_stats}")

    lod.triangulate(leaves_low)
    leaves1 = card_subset(leaves_low, 2, 1.25, "leaves_l1")
    leaves2 = card_subset(leaves_low, 4, 1.5, "leaves_l2")
    for o in (leaves1, leaves2):
        lod.triangulate(o)
    bark1, bark2 = copy_of(bark_low, "bark_l1"), copy_of(bark_low, "bark_l2")

    def lod_canopy(per_clump, name):
        ps = []
        for src in shell_srcs:
            o = copy_of(src, f"{src.name}_{name}")
            lod.decimate_to(o, per_clump)
            ps.append(o)
        return geom.join(ps, name)

    # Collider: hull of the leaning trunk's lowest 4 m, each 8-point ring centred on the strands' mean.
    ring_pts = []
    for zi, z in enumerate((0.0, 1.0, 2.0, 3.0, 3.6)):
        ctr = lean(z)
        rad = 0.0
        for b in trunk:
            for p, r in zip(b["points"], b["radii"]):
                if abs(p.z - z) < 0.3:
                    rad = max(rad, math.hypot(p.x - ctr.x, p.y - ctr.y) + r)
        ring_pts += [(ctr.x + rad * math.cos(2 * math.pi * (k + 0.5 * zi) / 8),
                      ctr.y + rad * math.sin(2 * math.pi * (k + 0.5 * zi) / 8), z) for k in range(8)]
    cbm = bmesh.new()
    for q in ring_pts:
        cbm.verts.new(q)
    bmesh.ops.convex_hull(cbm, input=cbm.verts)
    cme = bpy.data.meshes.new("collider_src")
    cbm.to_mesh(cme)
    cbm.free()
    csrc = scene.link(bpy.data.objects.new("collider_src", cme))
    col = collider.collider_from(csrc, "hull", f"{piece}_collider")
    bpy.data.objects.remove(csrc, do_unlink=True)
    assert len(col.data.vertices) <= 64, len(col.data.vertices)

    def at_scale(d):
        return {k: v / SCALE for k, v in d.items()}

    texel = {bark_mat.name: at_scale(uv.measure_texel_density(bark_low, TILE_UV_SCALE * BARK_SET_PX)),
             canopy_mat_name: at_scale(uv.measure_texel_density(canopy_low, ATLAS)),
             leaf_mat.name: at_scale(uv.measure_texel_density(leaves_low, ATLAS))}
    ao_density = at_scale(uv.measure_texel_density(bark_low, MASK_SIZE))

    lods = []
    parts0 = [geom.tri_count(o) for o in (bark_low, canopy_low, leaves_low)]
    t0 = sum(parts0)
    for i, (b, lv) in enumerate(((bark_low, leaves_low), (bark1, leaves1), (bark2, leaves2))):
        c = canopy_low
        if i:
            budget = int(t0 * (0.5 if i == 1 else 0.15)) - geom.tri_count(lv) - 4
            c = lod_canopy(int(budget * (0.55 if i == 1 else 0.6)) // len(crown), f"canopy_l{i}")
            lod.decimate_to(b, budget - geom.tri_count(c))
        o = geom.join([b, c, lv], f"{piece}_lod{i}")
        _tree.clamp_wind_uv(o)
        _oak.shell_normals(o, canopy_mat_name, crown)
        _tree.set_canopy_normals(o, leaf_mat.name, canopy_mat_name)
        _oak.junction_normals(o, skel, bark_mat.name)
        lods.append(o)
    for src in shell_srcs:
        bpy.data.objects.remove(src, do_unlink=True)
    tris = [geom.tri_count(o) for o in lods]
    height = max(v.co.z for v in lods[0].data.vertices)
    reach = {"ground": round(radial_max([col]) * SCALE, 3), "crown": round(radial_max(lods) * SCALE, 3),
             "crownPerLod": [round(radial_max([o]) * SCALE, 3) for o in lods], "height": round(height * SCALE, 3)}
    print(f"{piece}: LOD tris {tris}, LOD0 bark/canopy/fringe {parts0}, texel {texel}, bark AO texels "
          f"{ao_density}, bark v max {bark_vmax:.4f}, shell v max {shell_vmax:.4f}")
    print(f"{piece}: footprint at {SCALE}x {reach} (scatter FOOTPRINTS: ground = collider, crown = max over LODs)")
    print(f"{piece}: clumps at 1x (Rust OAK_CLUMPS form):")
    for cl in crown:
        c = cl["centre"]
        print(f"    ([{c.x:.3f}, {c.y:.3f}, {c.z:.3f}], {cl['radius']:.3f}, {cl['tier']}),")
    budgets.check(tris, "tree", piece)
    for name, t in texel.items():
        assert t["median"] >= 256, (name, t)

    for o in (*lods, col):
        keep = [tuple(c.vector) for c in o.data.corner_normals]
        o.data.transform(Matrix.Scale(SCALE, 4))
        if o is not col:
            o.data.normals_split_custom_set(keep)
        o.data.update()
        after = [tuple(c.vector) for c in o.data.corner_normals]
        diffs = [max(abs(a - b) for a, b in zip(u, v)) for u, v in zip(keep, after)]
        # Re-set custom normals round-trip through Blender's compressed corner-normal space: a few
        # canopy corners on this crown drift 2e-4 (the hero's stay under 1e-4), far below shading.
        bad = [i for i, d in enumerate(diffs) if d >= 1e-3]
        if bad:
            mats = o.data.materials
            polys = {lp: p for p in o.data.polygons for lp in p.loop_indices}
            print(f"{o.name}: {len(bad)} corners moved, max {max(diffs):.4f}; slots "
                  f"{sorted({mats[polys[i].material_index].name for i in bad})}; areas "
                  f"{sorted(round(polys[i].area, 6) for i in bad)[:8]}")
        assert not bad, o.name
    sig = bake.receiver_signature([lods[0]])

    root = scene.empty(piece)
    for o in lods:
        o.parent = root
    col.parent = root
    export.export_glb(root, str(out / f"{piece}.glb"))
    export.write_export_report(str(out / "export-report.json"), root, piece, generator, budget_class="hero")
    bake.write_bake_report(str(out / "bake-report.json"), receiverSignature=sig, samples=SAMPLES, marginPx=MARGIN,
                           cageExtrusionM=CAGE, maxRayDistanceM=MAX_RAY, aoDistanceM=AO_DIST, texelDensityPxPerM=texel,
                           scale=SCALE, footprint=reach,
                           canopyShell={"cageExtrusionM": SHELL_CAGE, "maxRayDistanceM": SHELL_RAY,
                                        "aoDistanceM": SHELL_AO_DIST, "sectors": _tree.SECTORS, "clumps": len(crown),
                                        "columnsPerSector": list(_oak.SHELL_COLUMNS),
                                        "ringLatitudes": list(_oak.SHELL_PHIS), "masterSeed": HERO,
                                        "uvArea": shell_area,
                                        "culledHiddenFaces": {"culled": culled_shell, "of": shell_faces}},
                           barkTile={"set": "bark", "tileUvScale": TILE_UV_SCALE, "uvPerM": BARK_UV_PER_M,
                                     "aoTexelDensityPxPerM": ao_density},
                           clumps=[list(c) for c in clumps], shape=shape,
                           channels={"albedo": {"size": ATLAS, "colorspace": "srgb"},
                                     "normal": {"size": ATLAS, "colorspace": "linear"},
                                     "orm": {"size": ATLAS, "colorspace": "linear"},
                                     "masks": {"size": MASK_SIZE, "colorspace": "linear"}},
                           device=os.environ.get("HEXW_BAKE_DEVICE", "CPU"), bakeStats=bake_stats,
                           culledReceiverFaces={"buried": culled, "inCrown": culled_crown,
                                                "underground": culled_ground, "of": faces_before},
                           lodPartsTris=parts0, skipped=skip_bake)
