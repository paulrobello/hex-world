"""Textured headless EEVEE stills of wildflower_single_e (evidence only: Blender's view transform is not
Bevy's). LOD0 of every single wildflower from assets/kit/p1 with its maps bound (albedo with alpha clip
0.5, ORM roughness and AO, the normal map) on a meadow-green ground: a lineup of wildflower_single,
_b, _c, _d and _e at true scale beside a 1.7 m figure, then a three-quarter, a low side, a top view and
a close-up of the poppy.
Usage (from the repo root):
  Blender -b --factory-startup --python-exit-code 1 -P docs/checkpoints/wildflower_single_e/still.py -- <out dir>"""
import json
import math
import pathlib
import sys

import bpy
from mathutils import Vector

ROOT = pathlib.Path(__file__).resolve().parents[3]
out_dir = pathlib.Path(sys.argv[sys.argv.index("--") + 1])
PIECES = ("wildflower_single", "wildflower_single_b", "wildflower_single_c", "wildflower_single_d",
          "wildflower_single_e")
NEW = "wildflower_single_e"
MANIFEST = {p["id"]: p for p in json.loads((ROOT / "assets/kit/manifest.json").read_text())["pieces"]}

bpy.ops.wm.read_factory_settings(use_empty=True)
scn = bpy.context.scene
scn.render.engine = "BLENDER_EEVEE"
scn.render.resolution_x, scn.render.resolution_y = 1200, 900
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


def bind(mat, maps, piece):
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
    gt = nt.nodes.new("ShaderNodeMath")
    gt.operation = "GREATER_THAN"
    gt.inputs[1].default_value = 0.5
    nt.links.new(alb.outputs["Alpha"], gt.inputs[0])
    nt.links.new(gt.outputs[0], bsdf.inputs["Alpha"])
    mat.surface_render_method = "DITHERED"
    mat.use_backface_culling = False


def load(piece):
    src = ROOT / "assets/kit/p1" / piece
    before = set(bpy.data.objects)
    bpy.ops.import_scene.gltf(filepath=str(src / f"{piece}.glb"))
    new = [o for o in bpy.data.objects if o not in before]
    keep = next(o for o in new if o.type == "MESH" and o.name.startswith(f"{piece}_lod0"))
    for o in new:
        if o is not keep and o.type == "MESH":
            bpy.data.objects.remove(o, do_unlink=True)
    keep.matrix_world = keep.matrix_world.copy()
    keep.parent = None
    for slot in keep.material_slots:
        if "alphaCutoff" in MANIFEST[piece]["slots"].get(slot.material.name, {}):
            bind(slot.material, src, piece)
    return keep


def shoot(name, loc, target, lens):
    cam.location = Vector(loc)
    cam.data.lens = lens
    cam.rotation_euler = (Vector(target) - cam.location).to_track_quat("-Z", "Y").to_euler()
    scn.render.filepath = str(out_dir / f"{name}.png")
    bpy.ops.render.render(write_still=True)


bpy.ops.mesh.primitive_plane_add(size=40.0, location=(0, 0, 0))
ground = bpy.context.object
gm = bpy.data.materials.new("meadow")
principled(gm).inputs["Base Color"].default_value = (0.16, 0.26, 0.07, 1)
principled(gm).inputs["Roughness"].default_value = 0.9
ground.data.materials.append(gm)
sun = bpy.data.objects.new("sun", bpy.data.lights.new("sun", "SUN"))
sun.data.energy = 4.0
sun.rotation_euler = (math.radians(48), 0, math.radians(35))
scn.collection.objects.link(sun)
cam = bpy.data.objects.new("cam", bpy.data.cameras.new("cam"))
scn.collection.objects.link(cam)
scn.camera = cam
bpy.ops.mesh.primitive_cylinder_add(radius=0.12, depth=1.7, location=(0, 0, 0.85))
fig = bpy.context.object
fm = bpy.data.materials.new("figure")
principled(fm).inputs["Base Color"].default_value = (0.25, 0.25, 0.3, 1)
fig.data.materials.append(fm)

objs = {p: load(p) for p in PIECES}


def show(only):
    for p, o in objs.items():
        o.hide_render = p not in only
    fig.hide_render = "fig" not in only


for i, (p, o) in enumerate(objs.items()):
    o.location = (-1.2 + 0.6 * i, 0, 0)
fig.location = (1.65, 0.25, 0.85)
show((*PIECES, "fig"))
shoot("wfe-lineup", (0.2, -3.4, 1.0), (0.2, 0.0, 0.6), 35)
objs[NEW].location = (0, 0, 0)
show((NEW,))
shoot(f"{NEW}-eevee-three_quarter", (0.9, -1.25, 1.05), (0, 0, 0.55), 40)
shoot(f"{NEW}-eevee-low_side", (1.5, -0.35, 0.45), (0, 0, 0.5), 40)
shoot(f"{NEW}-eevee-top", (0.15, -0.2, 2.0), (0, 0, 0.6), 40)
shoot(f"{NEW}-eevee-close", (0.35, -0.45, 0.95), (0, 0, 0.72), 40)
print("stills written", out_dir, PIECES)
