"""Granite boulders and the whaleback outcrop (B1, owner A25 2026-10-27; refs:
assets-source/refs/boulders, ref-1 the four boulders, ref-2 the outcrop): runtime scatter props in
rock_small's granite family, so their high poly and procedural granite come from `_rock`
(`Stone`, `build_high`, `granite_material`, `_downsample`), unchanged.

What differs from `_rock.build`, and why this file has its own pipeline:
- Size. 2.2 to 8 m across, so the unique bake holds about 128 px/m (owner A25), not the 256 px/m
  of the small rocks: 256 px/m on the big boulder or the outcrop needs a 2048² albedo, far over the
  1.5 MB piece KTX2 class. `texel_min` is that floor (the gate's scoped `BOULDER_TEXEL_MIN`).
- Relief at boulder scale (`lump`): `_rock.stone_bmesh` scales its lumps for 0.3-1 m stones, so a
  low-frequency lump of `lump` m is added on the high poly.
- Joints (`joints`): the outcrop's sheet joints (ref-2) are V-grooves carved into one shell, with
  a height step across each, never separate slabs (facing slab faces would bake AO 0).
- LOD2 (`lod2_tris` None): the two small boulders leave LOD2 entirely (owner A25: "past 400 m
  only _c, _d and the outcrop"), so their GLB has no `<id>_lod2` and the bake report's
  `lod2Drops` names their one slot with that ruling.
- Bake reach (`cage`, `max_ray`, `ao_dist`) and the AO ground plane scale with the piece.

LOD0 is the collapse-decimated high poly with one smart-project atlas, baked selected-to-active
(albedo through EMIT, tangent normal, AO with a ground plane into ORM.R, roughness into ORM.G, the
moss mask into masks.G); LOD1/LOD2 decimate LOD0 and keep its UVs."""
from __future__ import annotations

import math
import os
import pathlib
from dataclasses import dataclass

import bmesh
import bpy
import numpy as np
from mathutils import Vector, noise

from lib import bake, collider, export, geom, lod, materials, scene, uv

import _rock
from _rock import Stone

__all__ = ["Stone", "Joint", "build"]
SAMPLES, MARGIN = 32, 8
LOD2_DROP = ("owner A25 (2026-10-27): past 400 m only boulder_granite_c, boulder_granite_d and "
             "outcrop_granite are drawn; the two small boulders leave LOD2")


@dataclass
class Joint:
    """A sheet joint across the outcrop: a V-groove from a to b (Blender x, y, m), `depth` and
    half `width` (m), and a height `step` (m) the far side (left of a->b) stands above the near
    side, fading over `fade` m from the groove."""
    a: tuple[float, float]
    b: tuple[float, float]
    depth: float = 0.16
    width: float = 0.14
    step: float = 0.0
    fade: float = 3.0


def _seg(p: Vector, a: Vector, b: Vector) -> tuple[float, float, float]:
    """(distance from p to segment ab, signed side (+ left of a->b), t along ab clamped)."""
    ab = b - a
    t = max(0.0, min(1.0, (p - a).dot(ab) / ab.length_squared))
    q = a + ab * t
    side = ab.x * (p.y - a.y) - ab.y * (p.x - a.x)
    return (p - q).length, math.copysign(1.0, side), t


def shape_high(high: bpy.types.Object, lump: float, size: float, joints: list[Joint]) -> None:
    """Adds the boulder-scale lump and carves the joints on the high poly (z > -0.05 only, so the
    buried part keeps its shape)."""
    bm = bmesh.new()
    bm.from_mesh(high.data)
    if joints:
        bmesh.ops.subdivide_edges(bm, edges=bm.edges[:], cuts=1, use_grid_fill=True)
        bmesh.ops.triangulate(bm, faces=bm.faces[:])
    bm.normal_update()
    off = Vector((13.7, -4.1, 8.3))
    for v in bm.verts:
        if lump:
            v.co += v.normal * lump * noise.noise(v.co * (1.6 / size) + off)
    for j in joints:
        a, b = Vector(j.a), Vector(j.b)
        for v in bm.verts:
            if v.co.z < -0.05:
                continue
            d, side, t = _seg(Vector((v.co.x, v.co.y)), a, b)
            # Fade the groove out over the last 15% of the segment, so a joint dies out in the rock.
            end = min(1.0, min(t, 1.0 - t) / 0.15) if 0.0 < t < 1.0 else 0.0
            above = min(1.0, (v.co.z + 0.05) / 0.3)
            if d < j.width:
                v.co.z -= j.depth * (1.0 - d / j.width) ** 1.5 * end * above
            if j.step and side > 0 and d < j.fade:
                v.co.z += j.step * min(1.0, d / j.width) * (1.0 - d / j.fade) * end * above
    bm.to_mesh(high.data)
    bm.free()
    for p in high.data.polygons:
        p.use_smooth = True


MOSS_CUT = (0.53, 0.66)
# With less moss the granite reads lighter than rock_small_b (albedo luma 0.52 against 0.48): a
# small gain on the baked albedo keeps the set one family and nearer ref-1's grey.
ALBEDO_GAIN = 0.92


def sparser_moss(mat: bpy.types.Material, cut: tuple[float, float]) -> None:
    """Raises the moss noise threshold of `_rock.granite_material` (its 0.45-0.6 map range, the only
    one) to `cut`: on a boulder's broad tops the small rocks' moss read as green blotches, while
    ref-1 and ref-2 show lichen with moss only in the hollows."""
    hits = [n for n in mat.node_tree.nodes if n.type == "MAP_RANGE"
            and abs(n.inputs["From Min"].default_value - 0.45) < 1e-6
            and abs(n.inputs["From Max"].default_value - 0.6) < 1e-6]
    assert len(hits) == 1, len(hits)
    hits[0].inputs["From Min"].default_value, hits[0].inputs["From Max"].default_value = cut


def _islands(obj: bpy.types.Object, atlas: int) -> np.ndarray:
    """Texels (Blender pixel order, flat) whose centre lies inside a UV triangle of obj."""
    isl = np.zeros((atlas, atlas), bool)
    uvd = obj.data.uv_layers["UVMap"].data
    for poly in obj.data.polygons:
        tri = np.array([uvd[li].uv[:] for li in poly.loop_indices]) * atlas
        lo_, hi_ = np.floor(tri.min(0)).astype(int), np.ceil(tri.max(0)).astype(int)
        xs, ys = np.meshgrid(np.arange(lo_[0], hi_[0]) + 0.5, np.arange(lo_[1], hi_[1]) + 0.5)
        (ax, ay), (bx, by), (cx, cy) = tri[:3]
        d = (by - cy) * (ax - cx) + (cx - bx) * (ay - cy)
        if abs(d) < 1e-12:
            continue
        l1 = ((by - cy) * (xs - cx) + (cx - bx) * (ys - cy)) / d
        l2 = ((cy - ay) * (xs - cx) + (ax - cx) * (ys - cy)) / d
        inside = (l1 >= 0) & (l2 >= 0) & (l1 + l2 <= 1)
        isl[ys[inside].astype(int).clip(0, atlas - 1), xs[inside].astype(int).clip(0, atlas - 1)] = True
    return isl.ravel()


def _ground_slice(obj: bpy.types.Object) -> dict:
    """Extent of obj above z = 0 and the reach of its z = 0 slice (m)."""
    vs = [v.co for v in obj.data.vertices]
    up = [v for v in vs if v.z >= 0.0]
    near0 = [v for v in vs if abs(v.z) < 0.06] or up
    return {"sizeAboveGround": [round(max(v.x for v in up) - min(v.x for v in up), 3),
                                round(max(v.y for v in up) - min(v.y for v in up), 3),
                                round(max(v.z for v in vs), 3)],
            "groundReach": round(max(math.hypot(v.x, v.y) for v in near0), 3),
            "z": [round(min(v.z for v in vs), 3), round(max(v.z for v in vs), 3)]}


def build(piece: str, out: pathlib.Path, generator: str, *, stones: list[Stone], bury: float, lod_tris: tuple,
          atlas: int, normal_size: int, orm_size: int, mask_size: int, texel_min: float, lump: float = 0.0,
          joints: tuple = (), cage: float = 0.08, max_ray: float = 0.25, ao_dist: float = 0.5) -> None:
    """The B1 pipeline for `piece` (module docstring). lod_tris = (LOD0, LOD1, LOD2 or None)."""
    skip_bake = os.environ.get("HEXW_SKIP_BAKE") == "1"
    scene.reset_scene(piece)
    mat_name = f"{piece}_rock"
    high = _rock.build_high(piece, stones, piece, bury)
    size = max(max(s.semi[0], s.semi[1]) for s in stones)
    shape_high(high, lump, size, list(joints))
    granite = _rock.granite_material(f"{piece}_high")
    sparser_moss(granite, MOSS_CUT)
    high.data.materials.append(granite)
    lod0 = high.copy()
    lod0.data = high.data.copy()
    lod0.name = lod0.data.name = f"{piece}_lod0"
    scene.link(lod0)
    lod0.data.materials.clear()
    lod0.data.materials.append(materials.pbr_material(mat_name, materials.srgb("#85827b"), 0.75))
    lod.decimate_to(lod0, lod_tris[0])
    for p in lod0.data.polygons:
        p.use_smooth = True
    uv.atlas_unwrap([lod0], margin=0.006)
    texel = uv.measure_texel_density(lod0, atlas)
    shape = _ground_slice(lod0)
    print(f"{piece}: texel {texel}, shape {shape}")

    n_px = atlas * atlas
    albedo = np.zeros((n_px, 4), np.float32)
    albedo[:] = (*materials.srgb("#85827b")[:3], 1.0)
    normal = np.zeros((n_px, 4), np.float32)
    normal[:] = (0.5, 0.5, 1.0, 1.0)
    orm = np.zeros((n_px, 4), np.float32)
    orm[:] = (1.0, 0.75, 0.0, 1.0)
    moss_px = np.zeros(n_px, np.float32)
    bake_stats = {}
    if not skip_bake:
        bpy.context.scene.world = bpy.context.scene.world or bpy.data.worlds.new(f"{piece}_world")
        bake.setup_cycles(samples=SAMPLES)
        kw = dict(cage_extrusion=cage, max_ray=max_ray, margin=MARGIN)
        imgs = {k: bake.new_image(f"{piece}.{k}", atlas, non_color=k != "albedo")
                for k in ("albedo", "normal", "ao", "rough", "moss")}
        bake.bake_emit_input([high], lod0, "Base Color", imgs["albedo"], **kw)
        bake.bake_emit_input([high], lod0, "Emission Color", imgs["moss"], **kw)
        bake.bake_to(lod0, "NORMAL", imgs["normal"], highs=[high], **kw)
        bake.bake_to(lod0, "ROUGHNESS", imgs["rough"], highs=[high], **kw)
        g = size + 3.0
        gme = bpy.data.meshes.new("ground_occluder")
        gme.from_pydata([(-g, -g, 0), (g, -g, 0), (g, g, 0), (-g, g, 0)], [], [[0, 1, 2, 3]])
        ground = scene.link(bpy.data.objects.new("ground_occluder", gme))
        ground.data.materials.append(bpy.data.materials[f"{piece}_high"])
        bpy.context.scene.world.light_settings.distance = ao_dist
        bake.bake_to(lod0, "AO", imgs["ao"], highs=[high, ground], **kw)
        bpy.data.objects.remove(ground, do_unlink=True)
        bpy.context.view_layer.update()
        px = {}
        for k, im in imgs.items():
            p = bake.save_png(im, str(out / f"{piece}.{k}.tmp.png"))
            px[k] = bake.load_png(p)[1]
            os.remove(p)
        albedo[:] = px["albedo"]
        albedo[:, :3] *= ALBEDO_GAIN
        normal[:] = px["normal"]
        orm[:, 0] = px["ao"][:, 0]
        orm[:, 1] = px["rough"][:, 0]
        moss_px[:] = px["moss"][:, 0]
        for a in (albedo, normal, orm):
            a[:, 3] = 1.0
        flat = _islands(lod0, atlas)

        def st(a):
            return {"texels": int(a.size), "min": float(a.min()), "mean": float(a.mean()),
                    "p01": float(np.percentile(a, 1)), "p05": float(np.percentile(a, 5))}

        bake_stats["ao"] = st(orm[flat, 0])
        bake_stats["roughness"] = st(orm[flat, 1])
        bake_stats["albedoLuma"] = st(albedo[flat, :3].mean(1))
        bake_stats["mossFraction"] = float((moss_px[flat] > 0.5).mean())
        bake_stats["normalMissFraction"] = float((normal[flat, 2] < 0.05).mean())
    bake.write_png(atlas, albedo, str(out / f"{piece}.albedo.png"), non_color=False)
    nrm = normal if normal_size == atlas else _rock._downsample(normal, atlas, normal_size, renormal=True)
    bake.write_png(normal_size, nrm, str(out / f"{piece}.normal.png"))
    orm_out = orm if orm_size == atlas else _rock._downsample(orm, atlas, orm_size)
    bake.write_png(orm_size, orm_out, str(out / f"{piece}.orm.png"))
    f = atlas // mask_size
    moss_small = moss_px.reshape(mask_size, f, mask_size, f).mean(axis=(1, 3)).ravel()
    bake.make_masks(mask_size, 1.0, 0.35 + 0.45 * moss_small, 0.0, None, str(out / f"{piece}.masks.png"))
    bpy.data.objects.remove(high, do_unlink=True)
    bpy.context.view_layer.update()

    lods = [lod0]
    for i, target in ((1, lod_tris[1]), (2, lod_tris[2])):
        if target is None:
            continue
        o = lod0.copy()
        o.data = lod0.data.copy()
        o.name = o.data.name = f"{piece}_lod{i}"
        scene.link(o)
        lod.decimate_to(o, target)
        lods.append(o)
    for o in lods:
        lod.triangulate(o)
    tris = [geom.tri_count(o) for o in lods]
    ok = tris[0] <= 1500 and tris[1] <= 0.5 * tris[0] and (len(tris) < 3 or tris[2] <= 0.15 * tris[0])
    col = collider.collider_from(lods[-1], "hull", f"{piece}_collider")
    reach = max(math.hypot(v.co.x, v.co.y) for v in col.data.vertices)
    cx = [v.co.x for v in col.data.vertices]
    cy = [v.co.y for v in col.data.vertices]
    print(f"{piece}: tris per LOD {tris}, texel {texel}, collider reach {reach:.3f}, collider x "
          f"{min(cx):.3f}..{max(cx):.3f} y {min(cy):.3f}..{max(cy):.3f}, bake {bake_stats}")
    assert ok, tris
    assert texel["median"] >= texel_min, texel

    root = scene.empty(piece)
    for o in lods:
        o.parent = root
    col.parent = root
    export.export_glb(root, str(out / f"{piece}.glb"))
    export.write_export_report(str(out / "export-report.json"), root, piece, generator)
    bake.write_bake_report(str(out / "bake-report.json"), receiverSignature=bake.receiver_signature([lod0]),
                           device=os.environ.get("HEXW_BAKE_DEVICE", "CPU"), samples=SAMPLES, marginPx=MARGIN,
                           cageExtrusionM=cage, maxRayDistanceM=max_ray, aoDistanceM=ao_dist,
                           channels={"albedo": {"size": atlas, "colorspace": "srgb"},
                                     "normal": {"size": normal_size, "colorspace": "linear"},
                                     "orm": {"size": orm_size, "colorspace": "linear"},
                                     "masks": {"size": mask_size, "colorspace": "linear"}},
                           texelDensityPxPerM=texel, texelMinPxPerM=texel_min, bakeStats=bake_stats, shape=shape,
                           colliderReach=round(reach, 3),
                           stones=[{"semi": list(s.semi), "centre": list(s.centre)} for s in stones],
                           joints=[{"a": list(j.a), "b": list(j.b), "depth": j.depth, "step": j.step}
                                   for j in joints],
                           lod2Drops={} if lod_tris[2] is not None else {mat_name: LOD2_DROP},
                           skipped=skip_bake)
