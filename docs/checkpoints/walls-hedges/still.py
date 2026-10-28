"""Textured headless EEVEE stills of the B2 walls and hedges (evidence only: Blender's view transform is
not Bevy's). LOD0 of each piece with its maps bound (albedo, with alpha clip 0.5 on leaf cards, ORM
roughness and AO, the normal map; colour-only and tileable slots as their manifest colour), from
target/kit-stage/<id>/ when staged, else assets/kit/p1/<id>/, on a meadow-green ground beside a 1.7 m
figure. A lineup of each family, then a three-quarter, an eye-level (1.7 m) and a top view per piece.
Usage (from the repo root):
  Blender -b --factory-startup --python-exit-code 1 -P docs/checkpoints/walls-hedges/still.py -- <out dir> [piece ...]"""
import json
import math
import pathlib
import sys

import bpy
from mathutils import Vector

ROOT = pathlib.Path(__file__).resolve().parents[3]
args = sys.argv[sys.argv.index("--") + 1:]
out_dir = pathlib.Path(args[0])
out_dir.mkdir(parents=True, exist_ok=True)
WALLS = ("wall_run", "wall_run_b", "wall_end", "wall_bend", "wall_junction", "wall_gate")
HEDGES = ("hedge_run", "hedge_run_b", "hedge_end", "hedge_bend", "hedge_junction", "hedge_oak")
ONLY = set(args[1:])


def where(piece):
    stage = ROOT / "target/kit-stage" / piece
    if (stage / f"{piece}.glb").exists():
        return stage
    p1 = ROOT / "assets/kit/p1" / piece
    return p1 if (p1 / f"{piece}.glb").exists() else None


PIECES = [p for p in (*WALLS, *HEDGES) if where(p) and (not ONLY or p in ONLY)]
MANIFEST = {p["id"]: p for p in json.loads((ROOT / "assets/kit/manifest.json").read_text())["pieces"]}

bpy.ops.wm.read_factory_settings(use_empty=True)
scn = bpy.context.scene
scn.render.engine = "BLENDER_EEVEE"
scn.render.resolution_x, scn.render.resolution_y = 1400, 900
scn.render.use_stamp_date = False
scn.render.use_stamp_render_time = False
scn.view_settings.view_transform = "AgX"
world = bpy.data.worlds.new("still")
world.color = (0.45, 0.55, 0.7)
scn.world = world


def image(path, non_color):
    img = bpy.data.images.load(str(path))
    img.colorspace_settings.name = "Non-Color" if non_color else "sRGB"
    return img


def principled(mat):
    return next(n for n in mat.node_tree.nodes if n.type == "BSDF_PRINCIPLED")


def bind(mat, maps, piece, alpha):
    nt = mat.node_tree
    bsdf = principled(mat)
    alb = nt.nodes.new("ShaderNodeTexImage")
    alb.image = image(maps / f"{piece}.albedo.png", False)
    orm = nt.nodes.new("ShaderNodeTexImage")
    orm.image = image(maps / f"{piece}.orm.png", True)
    nrm = nt.nodes.new("ShaderNodeTexImage")
    nrm.image = image(maps / f"{piece}.normal.png", True)
    sep = nt.nodes.new("ShaderNodeSeparateColor")
    nt.links.new(orm.outputs["Color"], sep.inputs["Color"])
    ao = nt.nodes.new("ShaderNodeMix")
    ao.data_type = "RGBA"
    ao.blend_type = "MULTIPLY"
    ins = {x.identifier: x for x in ao.inputs}
    ins["Factor_Float"].default_value = 1.0
    nt.links.new(alb.outputs["Color"], ins["A_Color"])
    nt.links.new(sep.outputs["Red"], ins["B_Color"])
    nt.links.new(next(x for x in ao.outputs if x.identifier == "Result_Color"), bsdf.inputs["Base Color"])
    nt.links.new(sep.outputs["Green"], bsdf.inputs["Roughness"])
    nm = nt.nodes.new("ShaderNodeNormalMap")
    nt.links.new(nrm.outputs["Color"], nm.inputs["Color"])
    nt.links.new(nm.outputs["Normal"], bsdf.inputs["Normal"])
    if alpha:
        nt.links.new(alb.outputs["Alpha"], bsdf.inputs["Alpha"])
        mat.surface_render_method = "DITHERED"
        mat.use_backface_culling = False


def load(piece):
    maps = where(piece)
    before = set(bpy.data.objects)
    bpy.ops.import_scene.gltf(filepath=str(maps / f"{piece}.glb"))
    new = [o for o in bpy.data.objects if o not in before]
    keep = next(o for o in new if o.type == "MESH" and o.name.startswith(f"{piece}_lod0"))
    for o in new:
        if o is not keep and o.type == "MESH":
            bpy.data.objects.remove(o, do_unlink=True)
    keep.matrix_world = keep.matrix_world.copy()
    keep.parent = None
    for slot in keep.material_slots:
        mat = slot.material
        if mat.name.endswith("_leaves") or mat.name.endswith("_stone"):
            bind(mat, maps, piece, mat.name.endswith("_leaves"))
        else:
            info = MANIFEST.get(piece, {}).get("slots", {}).get(mat.name, {})
            default = (0.3, 0.26, 0.21) if mat.name.endswith("_timber") else (0.06, 0.05, 0.035)
            principled(mat).inputs["Base Color"].default_value = (*info.get("color", default), 1)
    return keep


def oak():
    """tree_oak_b, the hedgerow oak the runtime stands at a hedge_oak's pivot (committed GLB, untextured)."""
    before = set(bpy.data.objects)
    bpy.ops.import_scene.gltf(filepath=str(ROOT / "assets/kit/p1/tree_oak_b/tree_oak_b.glb"))
    new = [o for o in bpy.data.objects if o not in before]
    keep = next(o for o in new if o.type == "MESH" and o.name.startswith("tree_oak_b_lod0"))
    for o in new:
        if o is not keep and o.type == "MESH":
            bpy.data.objects.remove(o, do_unlink=True)
    keep.matrix_world = keep.matrix_world.copy()
    keep.parent = None
    for slot in keep.material_slots:
        b = principled(slot.material)
        b.inputs["Base Color"].default_value = (0.2, 0.15, 0.1, 1) if "bark" in slot.material.name else (0.06, 0.13, 0.03, 1)
    return keep


bpy.ops.mesh.primitive_plane_add(size=120.0, location=(0, 0, -0.01))
ground = bpy.context.object
gm = bpy.data.materials.new("ground")
principled(gm).inputs["Base Color"].default_value = (0.075, 0.16, 0.035, 1)
principled(gm).inputs["Roughness"].default_value = 0.9
ground.data.materials.append(gm)
sun = bpy.data.objects.new("sun", bpy.data.lights.new("sun", "SUN"))
sun.data.energy = 4.0
sun.rotation_euler = (math.radians(48), 0, math.radians(35))
scn.collection.objects.link(sun)
cam = bpy.data.objects.new("cam", bpy.data.cameras.new("cam"))
scn.collection.objects.link(cam)
scn.camera = cam
bpy.ops.mesh.primitive_cylinder_add(radius=0.18, depth=1.7, location=(0, 0, 0.85))
fig = bpy.context.object
fm = bpy.data.materials.new("figure")
principled(fm).inputs["Base Color"].default_value = (0.6, 0.1, 0.08, 1)
fig.data.materials.append(fm)

objs = {p: load(p) for p in PIECES}
tree = oak() if "hedge_oak" in objs else None


def shoot(name, loc, target, lens):
    cam.location = Vector(loc)
    cam.data.lens = lens
    cam.rotation_euler = (Vector(target) - cam.location).to_track_quat("-Z", "Y").to_euler()
    scn.render.filepath = str(out_dir / f"{name}.png")
    bpy.ops.render.render(write_still=True)


def show(only):
    for p, o in objs.items():
        o.hide_render = p not in only
    fig.hide_render = "fig" not in only
    if tree:
        tree.hide_render = "hedge_oak" not in only


for fam, group in (("walls", WALLS), ("hedges", HEDGES)):
    present = [p for p in group if p in objs]
    if not present:
        continue
    for i, p in enumerate(present):
        objs[p].location = (-11.0 + i * 5.2, 0, 0)
        objs[p].rotation_euler = (0, 0, 0)
    if tree and "hedge_oak" in present:
        tree.location = objs["hedge_oak"].location
    fig.location = (-11.0 + len(present) * 5.2 - 2.2, -1.6, 0.85)
    show((*present, "fig"))
    mid = -11.0 + (len(present) - 1) * 2.6
    shoot(f"{fam}-lineup", (mid, -31.0, 6.0), (mid, 0.0, 1.2), 33)
for p in PIECES:
    objs[p].location = (0, 0, 0)
    fig.location = (1.4, -1.9, 0.85)
    if tree and p == "hedge_oak":
        tree.location = (0, 0, 0)
    show((p, "fig"))
    shoot(f"{p}-eevee-three_quarter", (4.5, -6.5, 3.6), (0, 0, 0.7), 35)
    shoot(f"{p}-eevee-eye", (3.0, -4.2, 1.7), (0, 0, 0.9), 35)
    show((p,))
    shoot(f"{p}-eevee-top", (0.2, -0.3, 9.0), (0, 0, 0.0), 35)
print("stills written", out_dir, PIECES)
