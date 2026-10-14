"""rock_small: a weathered granite rock with moss and lichen, partly buried in the meadow turf
(refs: assets-source/refs/meadow_props). Scripted route (PRD §11.1), prop kind (LOD0 <= 1,500 tris).
Usage: make kit-build PIECE=rock_small.

The pivot is on the ground at the rock's centre (Blender z = 0 = runtime y = 0). The rock runs down
to z = BURY below the pivot, so it stays embedded where the meadow relief dips (to -0.25 m); faces
below that are never visible and are dropped. One piece: the kit has no variant mechanism, so
variety comes from the runtime's per-instance hue, scale and rotation jitter (PRD §6).

High poly: a displaced icosphere cleaved by seeded planes (granite blockiness), shaded procedurally
in object space (granite speckle, lichen rosettes, moss on up-facing faces, a damp soil line).
LOD0 is the collapse-decimated high poly with one smart-project atlas, baked selected-to-active:
albedo through EMIT, tangent normal, AO (with a ground plane as occluder) into ORM.R, roughness into
ORM.G, the moss mask into masks.G (porosity). masks.A = 1: the unique ORM carries the AO.
LOD1/LOD2 decimate LOD0 and keep its UVs."""
import math
import os
import pathlib
import random
import sys

import bmesh
import bpy
import numpy as np
from mathutils import Vector, noise

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
from lib import bake, collider, export, geom, lod, materials, scene, uv  # noqa: E402

PIECE, ATLAS, MASK_SIZE = "rock_small", 512, 128
SEMI = (0.62, 0.47, 0.40)  # ellipsoid semi-axes, m (about 1.2 x 0.9 m across, 0.35 m proud)
CENTRE_Z, BURY = -0.04, -0.27
LOD0_TRIS = 1100
CAGE, MAX_RAY, MARGIN, SAMPLES, AO_DIST = 0.04, 0.1, 8, 32, 0.25
MAT = f"{PIECE}_rock"
out = pathlib.Path(sys.argv[sys.argv.index("--") + 1])
skip_bake = os.environ.get("HEXW_SKIP_BAKE") == "1"
scene.reset_scene(PIECE)
rng = random.Random(PIECE)
noise.seed_set(7)


def build_high():
    """Displaced, plane-cleaved icosphere, smooth shaded, cut below BURY."""
    bm = bmesh.new()
    bmesh.ops.create_icosphere(bm, subdivisions=5, radius=1.0)
    off = Vector((rng.uniform(-50, 50), rng.uniform(-50, 50), rng.uniform(-50, 50)))
    # Cleave planes: mostly side- and top-facing, at 0.72-0.9 of the extent along their normal.
    planes = []
    for i in range(11):
        a = 2 * math.pi * (i + rng.uniform(-0.3, 0.3)) / 11
        el = rng.uniform(0.15, 0.75) if i % 3 else rng.uniform(0.9, 1.3)
        n = Vector((math.cos(a) * math.cos(el), math.sin(a) * math.cos(el), math.sin(el))).normalized()
        ext = math.sqrt(sum((s * c) ** 2 for s, c in zip(SEMI, n)))
        planes.append((n, ext * rng.uniform(0.70, 0.84)))
    for v in bm.verts:
        d = v.co.normalized()
        p = Vector((d.x * SEMI[0], d.y * SEMI[1], d.z * SEMI[2]))
        p += d * 0.10 * noise.noise(d * 1.4 + off)
        p.z += 0.10 * p.x / SEMI[0] + 0.04 * p.y / SEMI[1]  # lopsided: one shoulder higher (ref-2)
        for n, dist in planes:
            s = p.dot(n) - dist
            if s > 0:
                p -= n * s * 0.94
        v.co = p
    bmesh.ops.smooth_vert(bm, verts=bm.verts, factor=0.3, use_axis_x=True, use_axis_y=True, use_axis_z=True)
    bm.normal_update()
    for v in bm.verts:  # weathered surface: fine fractal relief along the normal
        v.co += v.normal * 0.018 * noise.fractal(v.co * 6.0 + off, 0.6, 2.0, 4)
    for v in bm.verts:
        v.co.z += CENTRE_Z
    bmesh.ops.delete(bm, geom=[f for f in bm.faces if all(v.co.z < BURY for v in f.verts)], context="FACES")
    bmesh.ops.delete(bm, geom=[v for v in bm.verts if not v.link_faces], context="VERTS")
    me = bpy.data.meshes.new(f"{PIECE}_high")
    bm.to_mesh(me)
    bm.free()
    for p in me.polygons:
        p.use_smooth = True
    return scene.link(bpy.data.objects.new(f"{PIECE}_high", me))


def granite_material(name):
    """Object-space procedural granite (the high poly sits at the origin, so object = world)."""
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
    # Granite: mottled grey, dark mica and pale feldspar specks.
    mottle = tex("ShaderNodeTexNoise", 3.0, 4.0)
    col = mix(rng_(mottle.outputs["Fac"], 0.35, 0.65), c("#8c8982"), c("#aba79e"))
    speck = tex("ShaderNodeTexNoise", 85.0, 1.0)
    col = mix(rng_(speck.outputs["Fac"], 0.36, 0.30), col, c("#3b3936"))
    col = mix(rng_(speck.outputs["Fac"], 0.64, 0.70), col, c("#c4bfb3"))
    # Weathering: dark streaks and stains from a stretched low-frequency noise.
    streak = N.new("ShaderNodeTexNoise")
    streak.inputs["Scale"].default_value, streak.inputs["Detail"].default_value = 6.0, 6.0
    sq = N.new("ShaderNodeVectorMath")
    sq.operation = "MULTIPLY"
    sq.inputs[1].default_value = (1.0, 1.0, 0.25)
    L.new(tc.outputs["Object"], sq.inputs[0])
    L.new(sq.outputs[0], streak.inputs["Vector"])
    col = mix(math_("MULTIPLY", rng_(streak.outputs["Fac"], 0.5, 0.7), 0.55), col, c("#5d5a54"))
    # Moss on up-facing faces, broken up by noise.
    up = N.new("ShaderNodeSeparateXYZ")
    L.new(geo.outputs["Normal"], up.inputs[0])
    moss_n = tex("ShaderNodeTexNoise", 5.0, 3.0)
    moss = math_("MULTIPLY", rng_(up.outputs["Z"], 0.45, 0.85), rng_(moss_n.outputs["Fac"], 0.45, 0.6))
    # Lichen rosettes: Voronoi cells, pale grey-green to chartreuse (ref-2), never under moss or soil.
    vor = tex("ShaderNodeTexVoronoi", 7.0)
    lich_n = tex("ShaderNodeTexNoise", 2.5, 2.0)
    lichen = math_("MULTIPLY", rng_(vor.outputs["Distance"], 0.42, 0.30), rng_(lich_n.outputs["Fac"], 0.36, 0.5))
    sep = N.new("ShaderNodeSeparateXYZ")
    L.new(tc.outputs["Object"], sep.inputs[0])
    above = rng_(sep.outputs["Z"], 0.02, 0.10)
    lichen = math_("MULTIPLY", math_("MULTIPLY", lichen, above), math_("SUBTRACT", 1.0, moss))
    ochre = tex("ShaderNodeTexNoise", 1.3, 1.0)
    lcol = mix(rng_(ochre.outputs["Fac"], 0.45, 0.55), c("#bcc07e"), c("#a3ad2e"))
    col = mix(lichen, col, lcol)
    mcol = mix(rng_(moss_n.outputs["Fac"], 0.5, 0.7), c("#4a6224"), c("#6f8434"))
    col = mix(math_("MULTIPLY", moss, 0.95), col, mcol)
    # Soil line: damp, dark and earthy where the rock meets (and runs under) the turf.
    soil = rng_(sep.outputs["Z"], 0.035, -0.01)
    col = mix(math_("MULTIPLY", soil, 0.8), col, c("#4a3d2c"))
    L.new(col, bsdf.inputs["Base Color"])
    # Roughness: granite 0.72, up to 0.95 under moss, 0.84 on lichen, wetter-looking soil line.
    r = N.new("ShaderNodeMath")
    r.operation = "ADD"
    r.inputs[1].default_value = 0.72
    L.new(math_("ADD", math_("MULTIPLY", moss, 0.23), math_("MULTIPLY", lichen, 0.12)), r.inputs[0])
    L.new(math_("MINIMUM", math_("ADD", r.outputs[0], math_("MULTIPLY", soil, 0.15)), 0.97), bsdf.inputs["Roughness"])
    # Speck relief for the normal bake (bump, small).
    bump = N.new("ShaderNodeBump")
    bump.inputs["Strength"].default_value = 0.25
    bump.inputs["Distance"].default_value = 0.004
    grain = tex("ShaderNodeTexNoise", 30.0, 2.0)
    L.new(grain.outputs["Fac"], bump.inputs["Height"])
    L.new(bump.outputs["Normal"], bsdf.inputs["Normal"])
    # Porosity source for masks.G: the moss mask on the (unlit) emission input.
    L.new(moss, bsdf.inputs["Emission Color"])
    bsdf.inputs["Emission Strength"].default_value = 0.0
    return m


high = build_high()
high.data.materials.append(granite_material(f"{PIECE}_high"))
lod0 = high.copy()
lod0.data = high.data.copy()
lod0.name = lod0.data.name = f"{PIECE}_lod0"
scene.link(lod0)
lod0.data.materials.clear()
rock_mat = materials.pbr_material(MAT, materials.srgb("#85827b"), 0.75)
lod0.data.materials.append(rock_mat)
lod.decimate_to(lod0, LOD0_TRIS)
for p in lod0.data.polygons:
    p.use_smooth = True
uv.atlas_unwrap([lod0], margin=0.008)
texel = uv.measure_texel_density(lod0, ATLAS)

albedo = np.zeros((ATLAS * ATLAS, 4), np.float32)
albedo[:] = (*materials.srgb("#85827b")[:3], 1.0)
normal = np.zeros((ATLAS * ATLAS, 4), np.float32)
normal[:] = (0.5, 0.5, 1.0, 1.0)
orm = np.zeros((ATLAS * ATLAS, 4), np.float32)
orm[:] = (1.0, 0.75, 0.0, 1.0)
moss_px = np.zeros(ATLAS * ATLAS, np.float32)
bake_stats = {}
if not skip_bake:
    bpy.context.scene.world = bpy.context.scene.world or bpy.data.worlds.new(f"{PIECE}_world")
    bake.setup_cycles(samples=SAMPLES)
    kw = dict(cage_extrusion=CAGE, max_ray=MAX_RAY, margin=MARGIN)
    imgs = {k: bake.new_image(f"{PIECE}.{k}", ATLAS, non_color=k != "albedo")
            for k in ("albedo", "normal", "ao", "rough", "moss")}
    bake.bake_emit_input([high], lod0, "Base Color", imgs["albedo"], **kw)
    bake.bake_emit_input([high], lod0, "Emission Color", imgs["moss"], **kw)
    bake.bake_to(lod0, "NORMAL", imgs["normal"], highs=[high], **kw)
    bake.bake_to(lod0, "ROUGHNESS", imgs["rough"], highs=[high], **kw)
    # AO with the turf as an occluder: a ground plane at z = 0 darkens the contact line.
    gme = bpy.data.meshes.new("ground_occluder")
    gme.from_pydata([(-3, -3, 0), (3, -3, 0), (3, 3, 0), (-3, 3, 0)], [], [[0, 1, 2, 3]])
    ground = scene.link(bpy.data.objects.new("ground_occluder", gme))
    ground.data.materials.append(bpy.data.materials[f"{PIECE}_high"])
    bpy.context.scene.world.light_settings.distance = AO_DIST
    bake.bake_to(lod0, "AO", imgs["ao"], highs=[high, ground], **kw)
    bpy.data.objects.remove(ground, do_unlink=True)
    bpy.context.view_layer.update()
    px = {}
    for k, im in imgs.items():
        p = bake.save_png(im, str(out / f"{PIECE}.{k}.tmp.png"))
        px[k] = bake.load_png(p)[1]
        os.remove(p)
    albedo[:] = px["albedo"]
    normal[:] = px["normal"]
    orm[:, 0] = px["ao"][:, 0]
    orm[:, 1] = px["rough"][:, 0]
    moss_px[:] = px["moss"][:, 0]
    for a in (albedo, normal, orm):
        a[:, 3] = 1.0
    # Island texels at texel centres (Blender pixel order), for AO / miss statistics.
    isl = np.zeros((ATLAS, ATLAS), bool)
    uvd = lod0.data.uv_layers["UVMap"].data
    for poly in lod0.data.polygons:
        tri = np.array([uvd[li].uv[:] for li in poly.loop_indices]) * ATLAS
        lo_, hi_ = np.floor(tri.min(0)).astype(int), np.ceil(tri.max(0)).astype(int)
        xs, ys = np.meshgrid(np.arange(lo_[0], hi_[0]) + 0.5, np.arange(lo_[1], hi_[1]) + 0.5)
        (ax, ay), (bx, by), (cx, cy) = tri
        d = (by - cy) * (ax - cx) + (cx - bx) * (ay - cy)
        if abs(d) < 1e-12:
            continue
        l1 = ((by - cy) * (xs - cx) + (cx - bx) * (ys - cy)) / d
        l2 = ((cy - ay) * (xs - cx) + (ax - cx) * (ys - cy)) / d
        inside = (l1 >= 0) & (l2 >= 0) & (l1 + l2 <= 1)
        isl[ys[inside].astype(int).clip(0, ATLAS - 1), xs[inside].astype(int).clip(0, ATLAS - 1)] = True
    flat = isl.ravel()

    def st(a):
        return {"texels": int(a.size), "min": float(a.min()), "mean": float(a.mean()),
                "p01": float(np.percentile(a, 1)), "p05": float(np.percentile(a, 5))}

    bake_stats["ao"] = st(orm[flat, 0])
    bake_stats["roughness"] = st(orm[flat, 1])
    bake_stats["albedoLuma"] = st(albedo[flat, :3].mean(1))
    bake_stats["mossFraction"] = float((moss_px[flat] > 0.5).mean())
    bake_stats["normalMissFraction"] = float((normal[flat, 2] < 0.05).mean())
bake.write_png(ATLAS, albedo, str(out / f"{PIECE}.albedo.png"), non_color=False)
bake.write_png(ATLAS, normal, str(out / f"{PIECE}.normal.png"))
bake.write_png(ATLAS, orm, str(out / f"{PIECE}.orm.png"))
# masks (linear): R snow capacity 1 (the shader adds up-facing coverage), G porosity (granite 0.35,
# moss 0.8), B no night emissive, A = 1 (AO is in the unique ORM.R).
f = ATLAS // MASK_SIZE
moss_small = moss_px.reshape(MASK_SIZE, f, MASK_SIZE, f).mean(axis=(1, 3)).ravel()
bake.make_masks(MASK_SIZE, 1.0, 0.35 + 0.45 * moss_small, 0.0, None, str(out / f"{PIECE}.masks.png"))
bpy.data.objects.remove(high, do_unlink=True)
bpy.context.view_layer.update()

# LODs: collapse-decimated copies of LOD0 (not border-touching), UVs carried over.
lods = [lod0]
t0 = geom.tri_count(lod0)
for i, ratio in ((1, 0.45), (2, 0.13)):
    o = lod0.copy()
    o.data = lod0.data.copy()
    o.name = o.data.name = f"{PIECE}_lod{i}"
    scene.link(o)
    lod.decimate_to(o, int(t0 * ratio))
    lods.append(o)
for o in lods:
    lod.triangulate(o)
tris = [geom.tri_count(o) for o in lods]
assert tris[0] <= 1500 and tris[1] <= 0.5 * tris[0] and tris[2] <= 0.15 * tris[0], tris
assert texel["median"] >= 256, texel
col = collider.collider_from(lods[2], "hull", f"{PIECE}_collider")
zs = [v.co.z for v in lod0.data.vertices]
print(f"{PIECE}: tris per LOD {tris}, texel {texel}, z {min(zs):.3f}..{max(zs):.3f}, bake {bake_stats}")

root = scene.empty(PIECE)
for o in lods:
    o.parent = root
col.parent = root
export.export_glb(root, str(out / f"{PIECE}.glb"))
export.write_export_report(str(out / "export-report.json"), root, PIECE, __file__)
bake.write_bake_report(str(out / "bake-report.json"), receiverSignature=bake.receiver_signature([lod0]),
                       device=os.environ.get("HEXW_BAKE_DEVICE", "CPU"), samples=SAMPLES, marginPx=MARGIN,
                       cageExtrusionM=CAGE, maxRayDistanceM=MAX_RAY, aoDistanceM=AO_DIST,
                       channels={"albedo": {"size": ATLAS, "colorspace": "srgb"},
                                 "normal": {"size": ATLAS, "colorspace": "linear"},
                                 "orm": {"size": ATLAS, "colorspace": "linear"},
                                 "masks": {"size": MASK_SIZE, "colorspace": "linear"}},
                       texelDensityPxPerM=texel, bakeStats=bake_stats, extentZ=[min(zs), max(zs)],
                       skipped=skip_bake)
