"""Rock variants of rock_small (PRD §6 Variety): the same weathered granite with moss and lichen
(refs: assets-source/refs/meadow_props), in other shapes. `rock_small.py` is frozen with the
approved original (provenance hashes it), so its high-poly recipe and its procedural granite are
carried over here unchanged in look, with the shape as parameters: one stone or a cluster of
stones (`Stone`), each a displaced icosphere cleaved by seeded planes, sunk to `bury` below the
pivot and cut there.

`build` is rock_small.py's pipeline: LOD0 is the collapse-decimated high poly with one
smart-project atlas, baked selected-to-active (albedo through EMIT, tangent normal, AO with a
ground plane as occluder into ORM.R, roughness into ORM.G, the moss mask into masks.G);
LOD1/LOD2 decimate LOD0 and keep its UVs. A larger stone needs a larger albedo to hold 256 px/m
(`atlas`), so the normal and ORM maps may ship smaller (`normal_size`, `orm_size`), box-filtered
from the full-size bake, as `wildflower_clump` ships its normal (project.md, Budgets)."""
from __future__ import annotations

import math
import os
import pathlib
import random
from dataclasses import dataclass

import bmesh
import bpy
import numpy as np
from mathutils import Vector, noise

from lib import bake, collider, export, geom, lod, materials, scene, uv

CAGE, MAX_RAY, MARGIN, SAMPLES, AO_DIST = 0.04, 0.1, 8, 32, 0.25


@dataclass
class Stone:
    """One stone: ellipsoid semi-axes (m), centre (x, y, z) and yaw (radians), its cleave planes'
    count and keep range (fraction of the extent along each plane normal), the top-plane elevation
    range (radians, higher = flatter top) and the shoulder tilt (rock_small: 0.10, 0.04)."""
    semi: tuple[float, float, float]
    centre: tuple[float, float, float] = (0.0, 0.0, 0.0)
    yaw: float = 0.0
    planes: int = 11
    keep: tuple[float, float] = (0.70, 0.84)
    top_el: tuple[float, float] = (0.9, 1.3)
    tilt: tuple[float, float] = (0.10, 0.04)


def stone_bmesh(bm: bmesh.types.BMesh, s: Stone, rng: random.Random, bury: float) -> None:
    """Adds one stone to bm: rock_small's displace, cleave, smooth and relief steps, then its yaw
    and centre, with faces wholly below `bury` dropped."""
    tmp = bmesh.new()
    bmesh.ops.create_icosphere(tmp, subdivisions=5, radius=1.0)
    off = Vector((rng.uniform(-50, 50), rng.uniform(-50, 50), rng.uniform(-50, 50)))
    planes = []
    for i in range(s.planes):
        a = 2 * math.pi * (i + rng.uniform(-0.3, 0.3)) / s.planes
        el = rng.uniform(0.15, 0.75) if i % 3 else rng.uniform(*s.top_el)
        n = Vector((math.cos(a) * math.cos(el), math.sin(a) * math.cos(el), math.sin(el))).normalized()
        ext = math.sqrt(sum((q * c) ** 2 for q, c in zip(s.semi, n)))
        planes.append((n, ext * rng.uniform(*s.keep)))
    for v in tmp.verts:
        d = v.co.normalized()
        p = Vector((d.x * s.semi[0], d.y * s.semi[1], d.z * s.semi[2]))
        p += d * 0.10 * min(1.0, s.semi[2] / 0.4) * noise.noise(d * 1.4 + off)
        p.z += s.tilt[0] * p.x / s.semi[0] + s.tilt[1] * p.y / s.semi[1]
        for n, dist in planes:
            k = p.dot(n) - dist
            if k > 0:
                p -= n * k * 0.94
        v.co = p
    bmesh.ops.smooth_vert(tmp, verts=tmp.verts, factor=0.3, use_axis_x=True, use_axis_y=True, use_axis_z=True)
    tmp.normal_update()
    for v in tmp.verts:
        v.co += v.normal * 0.018 * noise.fractal(v.co * 6.0 + off, 0.6, 2.0, 4)
    cy, sy = math.cos(s.yaw), math.sin(s.yaw)
    for v in tmp.verts:
        x, y, z = v.co
        v.co = Vector((cy * x - sy * y + s.centre[0], sy * x + cy * y + s.centre[1], z + s.centre[2]))
    bmesh.ops.delete(tmp, geom=[f for f in tmp.faces if all(v.co.z < bury for v in f.verts)], context="FACES")
    bmesh.ops.delete(tmp, geom=[v for v in tmp.verts if not v.link_faces], context="VERTS")
    me = bpy.data.meshes.new("stone_tmp")
    tmp.to_mesh(me)
    tmp.free()
    bm.from_mesh(me)
    bpy.data.meshes.remove(me)


def build_high(piece: str, stones: list[Stone], seed: str, bury: float) -> bpy.types.Object:
    rng = random.Random(seed)
    noise.seed_set(7)
    bm = bmesh.new()
    for s in stones:
        stone_bmesh(bm, s, rng, bury)
    me = bpy.data.meshes.new(f"{piece}_high")
    bm.to_mesh(me)
    bm.free()
    for p in me.polygons:
        p.use_smooth = True
    return scene.link(bpy.data.objects.new(f"{piece}_high", me))


def granite_material(name: str, soil_z: float = 0.0) -> bpy.types.Material:
    """rock_small's object-space procedural granite (the high poly sits at the origin, so object =
    world); `soil_z` moves its damp soil line and lichen floor with the ground."""
    m = bpy.data.materials.new(name)
    nt = materials.ensure_nodes(m)
    N, L = nt.nodes, nt.links
    bsdf = next(n for n in N if n.type == "BSDF_PRINCIPLED")
    tc = N.new("ShaderNodeTexCoord")
    geo = N.new("ShaderNodeNewGeometry")

    def tex(kind, scale, detail=2.0, **kw):
        t = N.new(kind)
        t.inputs["Scale"].default_value = scale
        if "Detail" in t.inputs:
            t.inputs["Detail"].default_value = detail
        for k, v in kw.items():
            t.inputs[k].default_value = v
        L.new(tc.outputs["Object"], t.inputs["Vector"])
        return t

    def rng_(src, a, b, lo=0.0, hi=1.0):
        r = N.new("ShaderNodeMapRange")
        r.inputs["From Min"].default_value, r.inputs["From Max"].default_value = a, b
        r.inputs["To Min"].default_value, r.inputs["To Max"].default_value = lo, hi
        r.clamp = True
        L.new(src, r.inputs["Value"])
        return r.outputs["Result"]

    def math_(op, a, b):
        n = N.new("ShaderNodeMath")
        n.operation = op
        for i, x in enumerate((a, b)):
            if isinstance(x, float):
                n.inputs[i].default_value = x
            else:
                L.new(x, n.inputs[i])
        return n.outputs[0]

    def mix(fac, a, b):
        n = N.new("ShaderNodeMix")
        n.data_type = "RGBA"
        ins = {s.identifier: s for s in n.inputs}
        L.new(fac, ins["Factor_Float"])
        for key, x in (("A_Color", a), ("B_Color", b)):
            if isinstance(x, tuple):
                ins[key].default_value = x
            else:
                L.new(x, ins[key])
        return next(s for s in n.outputs if s.identifier == "Result_Color")

    c = materials.srgb
    mottle = tex("ShaderNodeTexNoise", 3.0, 4.0)
    col = mix(rng_(mottle.outputs["Fac"], 0.35, 0.65), c("#8c8982"), c("#aba79e"))
    speck = tex("ShaderNodeTexNoise", 85.0, 1.0)
    col = mix(rng_(speck.outputs["Fac"], 0.36, 0.30), col, c("#3b3936"))
    col = mix(rng_(speck.outputs["Fac"], 0.64, 0.70), col, c("#c4bfb3"))
    streak = N.new("ShaderNodeTexNoise")
    streak.inputs["Scale"].default_value, streak.inputs["Detail"].default_value = 6.0, 6.0
    sq = N.new("ShaderNodeVectorMath")
    sq.operation = "MULTIPLY"
    sq.inputs[1].default_value = (1.0, 1.0, 0.25)
    L.new(tc.outputs["Object"], sq.inputs[0])
    L.new(sq.outputs[0], streak.inputs["Vector"])
    col = mix(math_("MULTIPLY", rng_(streak.outputs["Fac"], 0.5, 0.7), 0.55), col, c("#5d5a54"))
    up = N.new("ShaderNodeSeparateXYZ")
    L.new(geo.outputs["Normal"], up.inputs[0])
    moss_n = tex("ShaderNodeTexNoise", 5.0, 3.0)
    moss = math_("MULTIPLY", rng_(up.outputs["Z"], 0.45, 0.85), rng_(moss_n.outputs["Fac"], 0.45, 0.6))
    vor = tex("ShaderNodeTexVoronoi", 7.0)
    lich_n = tex("ShaderNodeTexNoise", 2.5, 2.0)
    lichen = math_("MULTIPLY", rng_(vor.outputs["Distance"], 0.42, 0.30), rng_(lich_n.outputs["Fac"], 0.36, 0.5))
    sep = N.new("ShaderNodeSeparateXYZ")
    L.new(tc.outputs["Object"], sep.inputs[0])
    above = rng_(sep.outputs["Z"], soil_z + 0.02, soil_z + 0.10)
    lichen = math_("MULTIPLY", math_("MULTIPLY", lichen, above), math_("SUBTRACT", 1.0, moss))
    ochre = tex("ShaderNodeTexNoise", 1.3, 1.0)
    lcol = mix(rng_(ochre.outputs["Fac"], 0.45, 0.55), c("#bcc07e"), c("#a3ad2e"))
    col = mix(lichen, col, lcol)
    mcol = mix(rng_(moss_n.outputs["Fac"], 0.5, 0.7), c("#4a6224"), c("#6f8434"))
    col = mix(math_("MULTIPLY", moss, 0.95), col, mcol)
    soil = rng_(sep.outputs["Z"], soil_z + 0.035, soil_z - 0.01)
    col = mix(math_("MULTIPLY", soil, 0.8), col, c("#4a3d2c"))
    L.new(col, bsdf.inputs["Base Color"])
    r = N.new("ShaderNodeMath")
    r.operation = "ADD"
    r.inputs[1].default_value = 0.72
    L.new(math_("ADD", math_("MULTIPLY", moss, 0.23), math_("MULTIPLY", lichen, 0.12)), r.inputs[0])
    L.new(math_("MINIMUM", math_("ADD", r.outputs[0], math_("MULTIPLY", soil, 0.15)), 0.97), bsdf.inputs["Roughness"])
    bump = N.new("ShaderNodeBump")
    bump.inputs["Strength"].default_value = 0.25
    bump.inputs["Distance"].default_value = 0.004
    grain = tex("ShaderNodeTexNoise", 30.0, 2.0)
    L.new(grain.outputs["Fac"], bump.inputs["Height"])
    L.new(bump.outputs["Normal"], bsdf.inputs["Normal"])
    L.new(moss, bsdf.inputs["Emission Color"])
    bsdf.inputs["Emission Strength"].default_value = 0.0
    return m


def _downsample(px: np.ndarray, size: int, out: int, renormal: bool = False) -> np.ndarray:
    """Box-filters (size*size, 4) Blender-order pixels to out x out; renormal re-normalizes RGB as
    a tangent normal."""
    f = size // out
    a = px.reshape(out, f, out, f, 4).mean(axis=(1, 3))
    if renormal:
        n = a[..., :3] * 2.0 - 1.0
        n /= np.maximum(np.linalg.norm(n, axis=-1, keepdims=True), 1e-6)
        a[..., :3] = n * 0.5 + 0.5
    a[..., 3] = 1.0
    return a.reshape(-1, 4).astype(np.float32)


def build(piece: str, out: pathlib.Path, generator: str, *, stones: list[Stone], bury: float, lod0_tris: int,
          atlas: int, normal_size: int, orm_size: int, mask_size: int, collider_kind: str = "hull") -> None:
    """rock_small.py's pipeline for variant `piece` (see the module docstring)."""
    skip_bake = os.environ.get("HEXW_SKIP_BAKE") == "1"
    scene.reset_scene(piece)
    mat_name = f"{piece}_rock"
    high = build_high(piece, stones, piece, bury)
    high.data.materials.append(granite_material(f"{piece}_high"))
    lod0 = high.copy()
    lod0.data = high.data.copy()
    lod0.name = lod0.data.name = f"{piece}_lod0"
    scene.link(lod0)
    lod0.data.materials.clear()
    lod0.data.materials.append(materials.pbr_material(mat_name, materials.srgb("#85827b"), 0.75))
    lod.decimate_to(lod0, lod0_tris)
    for p in lod0.data.polygons:
        p.use_smooth = True
    uv.atlas_unwrap([lod0], margin=0.008)
    texel = uv.measure_texel_density(lod0, atlas)

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
        kw = dict(cage_extrusion=CAGE, max_ray=MAX_RAY, margin=MARGIN)
        imgs = {k: bake.new_image(f"{piece}.{k}", atlas, non_color=k != "albedo")
                for k in ("albedo", "normal", "ao", "rough", "moss")}
        bake.bake_emit_input([high], lod0, "Base Color", imgs["albedo"], **kw)
        bake.bake_emit_input([high], lod0, "Emission Color", imgs["moss"], **kw)
        bake.bake_to(lod0, "NORMAL", imgs["normal"], highs=[high], **kw)
        bake.bake_to(lod0, "ROUGHNESS", imgs["rough"], highs=[high], **kw)
        gme = bpy.data.meshes.new("ground_occluder")
        gme.from_pydata([(-4, -4, 0), (4, -4, 0), (4, 4, 0), (-4, 4, 0)], [], [[0, 1, 2, 3]])
        ground = scene.link(bpy.data.objects.new("ground_occluder", gme))
        ground.data.materials.append(bpy.data.materials[f"{piece}_high"])
        bpy.context.scene.world.light_settings.distance = AO_DIST
        bake.bake_to(lod0, "AO", imgs["ao"], highs=[high, ground], **kw)
        bpy.data.objects.remove(ground, do_unlink=True)
        bpy.context.view_layer.update()
        px = {}
        for k, im in imgs.items():
            p = bake.save_png(im, str(out / f"{piece}.{k}.tmp.png"))
            px[k] = bake.load_png(p)[1]
            os.remove(p)
        albedo[:] = px["albedo"]
        normal[:] = px["normal"]
        orm[:, 0] = px["ao"][:, 0]
        orm[:, 1] = px["rough"][:, 0]
        moss_px[:] = px["moss"][:, 0]
        for a in (albedo, normal, orm):
            a[:, 3] = 1.0
        isl = np.zeros((atlas, atlas), bool)
        uvd = lod0.data.uv_layers["UVMap"].data
        for poly in lod0.data.polygons:
            tri = np.array([uvd[li].uv[:] for li in poly.loop_indices]) * atlas
            lo_, hi_ = np.floor(tri.min(0)).astype(int), np.ceil(tri.max(0)).astype(int)
            xs, ys = np.meshgrid(np.arange(lo_[0], hi_[0]) + 0.5, np.arange(lo_[1], hi_[1]) + 0.5)
            (ax, ay), (bx, by), (cx, cy) = tri
            d = (by - cy) * (ax - cx) + (cx - bx) * (ay - cy)
            if abs(d) < 1e-12:
                continue
            l1 = ((by - cy) * (xs - cx) + (cx - bx) * (ys - cy)) / d
            l2 = ((cy - ay) * (xs - cx) + (ax - cx) * (ys - cy)) / d
            inside = (l1 >= 0) & (l2 >= 0) & (l1 + l2 <= 1)
            isl[ys[inside].astype(int).clip(0, atlas - 1), xs[inside].astype(int).clip(0, atlas - 1)] = True
        flat = isl.ravel()

        def st(a):
            return {"texels": int(a.size), "min": float(a.min()), "mean": float(a.mean()),
                    "p01": float(np.percentile(a, 1)), "p05": float(np.percentile(a, 5))}

        bake_stats["ao"] = st(orm[flat, 0])
        bake_stats["roughness"] = st(orm[flat, 1])
        bake_stats["albedoLuma"] = st(albedo[flat, :3].mean(1))
        bake_stats["mossFraction"] = float((moss_px[flat] > 0.5).mean())
        bake_stats["normalMissFraction"] = float((normal[flat, 2] < 0.05).mean())
    bake.write_png(atlas, albedo, str(out / f"{piece}.albedo.png"), non_color=False)
    nrm = normal if normal_size == atlas else _downsample(normal, atlas, normal_size, renormal=True)
    bake.write_png(normal_size, nrm, str(out / f"{piece}.normal.png"))
    orm_out = orm if orm_size == atlas else _downsample(orm, atlas, orm_size)
    bake.write_png(orm_size, orm_out, str(out / f"{piece}.orm.png"))
    f = atlas // mask_size
    moss_small = moss_px.reshape(mask_size, f, mask_size, f).mean(axis=(1, 3)).ravel()
    bake.make_masks(mask_size, 1.0, 0.35 + 0.45 * moss_small, 0.0, None, str(out / f"{piece}.masks.png"))
    bpy.data.objects.remove(high, do_unlink=True)
    bpy.context.view_layer.update()

    lods = [lod0]
    t0 = geom.tri_count(lod0)
    for i, ratio in ((1, 0.45), (2, 0.13)):
        o = lod0.copy()
        o.data = lod0.data.copy()
        o.name = o.data.name = f"{piece}_lod{i}"
        scene.link(o)
        lod.decimate_to(o, int(t0 * ratio))
        lods.append(o)
    for o in lods:
        lod.triangulate(o)
    tris = [geom.tri_count(o) for o in lods]
    assert tris[0] <= 1500 and tris[1] <= 0.5 * tris[0] and tris[2] <= 0.15 * tris[0], tris
    assert texel["median"] >= 256, texel
    col = collider.collider_from(lods[2], collider_kind, f"{piece}_collider")
    zs = [v.co.z for v in lod0.data.vertices]
    reach = max(math.hypot(v.co.x, v.co.y) for v in col.data.vertices)
    print(f"{piece}: tris per LOD {tris}, texel {texel}, z {min(zs):.3f}..{max(zs):.3f}, collider reach "
          f"{reach:.3f}, bake {bake_stats}")

    root = scene.empty(piece)
    for o in lods:
        o.parent = root
    col.parent = root
    export.export_glb(root, str(out / f"{piece}.glb"))
    export.write_export_report(str(out / "export-report.json"), root, piece, generator)
    bake.write_bake_report(str(out / "bake-report.json"), receiverSignature=bake.receiver_signature([lod0]),
                           device=os.environ.get("HEXW_BAKE_DEVICE", "CPU"), samples=SAMPLES, marginPx=MARGIN,
                           cageExtrusionM=CAGE, maxRayDistanceM=MAX_RAY, aoDistanceM=AO_DIST,
                           channels={"albedo": {"size": atlas, "colorspace": "srgb"},
                                     "normal": {"size": normal_size, "colorspace": "linear"},
                                     "orm": {"size": orm_size, "colorspace": "linear"},
                                     "masks": {"size": mask_size, "colorspace": "linear"}},
                           texelDensityPxPerM=texel, bakeStats=bake_stats, extentZ=[min(zs), max(zs)],
                           stones=[{"semi": list(s.semi), "centre": list(s.centre)} for s in stones],
                           skipped=skip_bake)
