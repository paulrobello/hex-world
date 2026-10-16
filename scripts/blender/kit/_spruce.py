"""Spruce variants of tree_forest (PRD §6 Variety): the same Norway spruce, bark set and needle tiles
as the approved `tree_forest`, with another silhouette. Refs: assets-source/refs/forest.

`tree_forest.py`, `_forest.py` and `_tree.py` are frozen with the approved original (provenance
hashes them), so this module runs the original's pipeline in `build`, unchanged in its steps, and
sets the silhouette through `_forest`'s module globals, which its functions read at call time
(`configure`). The frond tiles are drawn with the original's seed and palette, so every spruce
shows the same needles; the whorls, card phases and jitter use the variant's own seed.

Differences from tree_forest.py: the trunk skeleton scales the base flare with the trunk radius
(a thin young trunk would otherwise get the old tree's 0.16 m flare), the bark uvPerM is a
parameter (a taller trunk needs a smaller one to fit below NEEDLE_V0, and the manifest
`tileUvScale` follows it), and the LOD table is a parameter (a short tree's fixed trunk and core
cost needs a sparser LOD2 to stay under 15%)."""
from __future__ import annotations

import math
import os
import pathlib

import bmesh
import bpy
import numpy as np
from mathutils import Vector

from lib import bake, collider, export, geom, lod, materials, scene, uv
import _forest as F
from _shared import island_texels  # noqa: E402
import _tree

ORIGINAL = "tree_forest"  # the frond tiles' seed: every spruce shows the original's needles
MASK_SIZE = 1024
BARK_SET_PX, BARK_TILE_M = 1024, 2.0  # the `bark` tileable set (scripts/blender/kit/mat_bark.py)
BARK_BOX_U = 0.5
CORE_BOX = (0.55, 0.01, 0.98, 0.26)
CAGE, MAX_RAY, MARGIN, SAMPLES = 0.12, 0.3, 8, 32
AO_DIST = 0.3


def configure(piece: str, **shape) -> None:
    """Point `_forest` at variant `piece` with the given silhouette globals (HEIGHT, Z0, ZTOP,
    WHORLS, RMAX, TRUNK_R0, TRUNK_R1). Only existing globals may be set."""
    F.PIECE = piece
    F.SEED = piece
    for k, v in shape.items():
        assert hasattr(F, k), k
        setattr(F, k, v)


def trunk_skeleton(flare: float) -> list[dict]:
    """`_forest.trunk_skeleton` with the base flare as a parameter (the original's is 0.16 m)."""
    pts, radii = [], []
    n = 33
    for i in range(n):
        t = i / (n - 1)
        z = -0.15 + (F.HEIGHT + 0.15) * t
        pts.append(Vector((0.03 * math.sin(2.1 * t), 0.02 * math.sin(3.3 * t + 1), z)))
        radii.append(F.TRUNK_R1 + (F.TRUNK_R0 - F.TRUNK_R1) * (1 - t) ** 1.15
                     + flare * max(0.0, 1 - z / 0.9) ** 2)
    return [{"id": 0, "level": 0, "points": pts, "radii": radii, "sway": (0.0, F.TRUNK_SWAY)}]


def build(piece: str, out: pathlib.Path, generator: str, *, shape: dict, flare: float, bark_uv_per_m: float,
          lods: tuple[dict, ...]) -> None:
    """tree_forest.py's pipeline for variant `piece`: geometry, frond atlas, trunk AO bake into
    masks, LOD joins, collider, export and reports into `out`. `generator` is the variant's
    `__file__` (the export report hashes it and its helpers)."""
    configure(piece, **shape)
    atlas = F.ATLAS
    tile_uv_scale = 1 / (bark_uv_per_m * BARK_TILE_M)
    skip_bake = os.environ.get("HEXW_SKIP_BAKE") == "1"
    scene.reset_scene(piece)
    scn = bpy.context.scene
    scn.world = scn.world or bpy.data.worlds.new(piece)
    bark_mat = materials.pbr_material(f"{piece}_bark", materials.srgb("#6a4e3e"), 0.85)
    needle_mat = materials.pbr_material(f"{piece}_needles", materials.srgb("#2e5226"), 0.65)
    core_mat = materials.pbr_material(f"{piece}_core", materials.srgb("#0c180a"), 0.9)
    skel = trunk_skeleton(flare)
    cards = F.branches()
    mains = [b for b in cards if b["kind"] == "main"]
    fillers = [b for b in cards if b["kind"] == "filler"]

    def trunk(name, sides, step, uv_off=None):
        t = _tree.tube_mesh(name, skel, {0: sides}, {0: step})
        me = t.data
        a = np.empty(len(me.loops) * 2)
        me.uv_layers["UVMap"].data.foreach_get("uv", a)
        a *= bark_uv_per_m
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
    for i, c in enumerate(lods):
        tr, uv_off = trunk(f"trunk_l{i}", c["sides"], c["step"], uv_off)
        core = F.core_mesh(f"core_l{i}", *c["core"], CORE_BOX)
        core.data.materials.append(core_mat)
        parts = [F.card_mesh(f"cards_l{i}", mains[::c["every"]], c["segs"], c["folded"], c["width"], c["leader"])]
        if c["fsegs"]:
            parts.append(F.card_mesh(f"fillers_l{i}", fillers, c["fsegs"], c["ffolded"], 1.0, leader=0))
        for p in parts:
            p.data.materials.append(needle_mat)
        lod_parts.append((tr, core, parts))

    needles0 = geom.join(lod_parts[0][2], "needles_l0")
    lod_parts[0] = (lod_parts[0][0], lod_parts[0][1], [needles0])
    texel = {f"{piece}_bark": uv.measure_texel_density(lod_parts[0][0], tile_uv_scale * BARK_SET_PX),
             f"{piece}_needles": uv.measure_texel_density(needles0, atlas)}
    ao_density = uv.measure_texel_density(lod_parts[0][0], MASK_SIZE)
    for name, t in texel.items():
        assert t["median"] >= 256, (name, t)

    # Frond strip with the original's seed: the same needle tiles as tree_forest.
    F.SEED = ORIGINAL
    strip_alb, strip_nrm, strip_rough = F.frond_strip()
    F.SEED = piece
    assert strip_alb.shape[:2] == (atlas - F.NEEDLE_ROWS0, atlas), strip_alb.shape
    coverage = F.coverage_by_mip(strip_alb[..., 3])
    albedo = np.empty((atlas, atlas, 4), np.float32)
    albedo[:] = (*strip_alb[..., :3][strip_alb[..., 3] > 0.5].mean(0), 1.0)
    albedo[F.NEEDLE_ROWS0:] = strip_alb
    normal = np.empty((atlas, atlas, 4), np.float32)
    normal[:] = (0.5, 0.5, 1.0, 1.0)
    normal[F.NEEDLE_ROWS0:, :, :3] = strip_nrm
    orm = np.empty((atlas, atlas, 4), np.float32)
    orm[:] = (1.0, 0.65, 0.0, 1.0)
    orm[F.NEEDLE_ROWS0:, :, 1] = strip_rough

    mk = np.empty((MASK_SIZE, MASK_SIZE, 4), np.float32)
    mk[:] = (1.0, 0.5, 0.0, 1.0)
    cu0, cv0, cu1, cv1 = (int(round(x * MASK_SIZE)) for x in CORE_BOX)
    rows = np.linspace(0.0, 1.0, cv1 - cv0)[:, None]
    mk[cv0:cv1, cu0:cu1, 3] = 0.5 + 0.35 * rows
    mk[cv0:cv1, cu0:cu1, 0] = 0.6
    bake_stats = {"needleAlphaCoverageByMip": coverage}

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
        ao_img = bake.new_image(f"{piece}.ao", MASK_SIZE, non_color=True)
        bake.bake_to(trunk0, "AO", ao_img, highs=[bark_high], cage_extrusion=CAGE, max_ray=MAX_RAY, margin=MARGIN)
        ao_tmp = bake.save_png(ao_img, str(out / f"{piece}.ao.tmp.png"))
        _, ao_px = bake.load_png(ao_tmp)
        os.remove(ao_tmp)
        ao = ao_px[:, 0].reshape(MASK_SIZE, MASK_SIZE).copy()
        # The original's analytic ground contact and crown shade (tree_forest.py), on this trunk.
        v = (np.arange(MASK_SIZE) + 0.5) / MASK_SIZE
        z = (v - uv_off[1]) / bark_uv_per_m - skel[0]["radii"][0] * 0.3 - 0.15
        s1 = np.clip(z / 1.2, 0, 1)
        s2 = np.clip((z - (F.Z0 - 0.5)) / 1.0, 0, 1)
        shade = (0.72 + 0.28 * s1 * s1 * (3 - 2 * s1)) * (1 - 0.15 * s2 * s2 * (3 - 2 * s2))
        ao *= shade[:, None]
        mk[..., 3] = np.where(grow, ao, mk[..., 3])
        a = ao[isl]
        bake_stats["barkAo"] = {"texels": int(a.size), "min": float(a.min()), "mean": float(a.mean()),
                                "p01": float(np.percentile(a, 1)), "p05": float(np.percentile(a, 5))}
        bpy.data.objects.remove(bark_high, do_unlink=True)
    bake.write_png(MASK_SIZE, mk.reshape(-1, 4), str(out / f"{piece}.masks.png"))
    bake.write_png(atlas, albedo.reshape(-1, 4), str(out / f"{piece}.albedo.png"), non_color=False)
    bake.write_png(atlas, normal.reshape(-1, 4), str(out / f"{piece}.normal.png"))
    bake.write_png(atlas, orm.reshape(-1, 4), str(out / f"{piece}.orm.png"))
    print(f"{piece}: bake stats {bake_stats}")

    lod_objs = []
    for i, (tr, core, parts) in enumerate(lod_parts):
        o = geom.join([tr, core, *parts], f"{piece}_lod{i}")
        lod.triangulate(o)
        _tree.clamp_wind_uv(o)
        F.set_spruce_normals(o, needle_mat.name, core_mat.name)
        lod_objs.append(o)
    tris = [geom.tri_count(o) for o in lod_objs]
    assert tris[0] <= 3000 and tris[1] <= 0.5 * tris[0] and tris[2] <= 0.15 * tris[0], tris
    zmin = min(v.co.z for o in lod_objs for p in o.data.polygons
               if o.data.materials[p.material_index].name == needle_mat.name
               for v in (o.data.vertices[k] for k in p.vertices))
    assert zmin > 0.6, zmin
    reach = max(math.hypot(v.co.x, v.co.y) for o in lod_objs for v in o.data.vertices)
    print(f"{piece}: tris per LOD {tris}, cards {len(mains)} main + {len(fillers)} filler, lowest card z {zmin:.2f}, "
          f"crown reach {reach:.3f}, texel {texel}, bark AO texels {ao_density}")

    ring_pts = []
    for zi, z in enumerate((0.0, 1.0, 2.0, 3.0)):
        p = min(skel[0]["points"], key=lambda q: abs(q.z - z))
        r = skel[0]["radii"][skel[0]["points"].index(p)]
        ring_pts += [(p.x + r * math.cos(2 * math.pi * (k + 0.5 * zi) / 8),
                      p.y + r * math.sin(2 * math.pi * (k + 0.5 * zi) / 8), z) for k in range(8)]
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

    root = scene.empty(piece)
    for o in lod_objs:
        o.parent = root
    col.parent = root
    sig = bake.receiver_signature([lod_objs[0]])
    export.export_glb(root, str(out / f"{piece}.glb"))
    export.write_export_report(str(out / "export-report.json"), root, piece, generator, budget_class="piece")
    bake.write_bake_report(str(out / "bake-report.json"), receiverSignature=sig, samples=SAMPLES, marginPx=MARGIN,
                           cageExtrusionM=CAGE, maxRayDistanceM=MAX_RAY, aoDistanceM=AO_DIST, texelDensityPxPerM=texel,
                           barkTile={"set": "bark", "tileUvScale": tile_uv_scale, "uvPerM": bark_uv_per_m,
                                     "aoTexelDensityPxPerM": ao_density},
                           cards={"main": len(mains), "filler": len(fillers), "frondTiles": F.FROND_TILES},
                           shape={**shape, "flare": flare, "original": ORIGINAL},
                           channels={"albedo": {"size": atlas, "colorspace": "srgb"},
                                     "normal": {"size": atlas, "colorspace": "linear"},
                                     "orm": {"size": atlas, "colorspace": "linear"},
                                     "masks": {"size": MASK_SIZE, "colorspace": "linear"}},
                           device=os.environ.get("HEXW_BAKE_DEVICE", "CPU"), bakeStats=bake_stats, skipped=skip_bake)
