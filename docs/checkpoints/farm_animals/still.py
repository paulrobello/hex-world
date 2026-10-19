"""Textured headless EEVEE stills of a staged farm animal (evidence only: Blender's view transform is not
Bevy's): LOD0 from target/kit-stage/<piece>/<piece>.glb with its stage PNGs bound (albedo x ORM AO, ORM
roughness, the normal map) on a grass-coloured ground, from the left side (the reference's view), the
three-quarter front and the three-quarter back. Usage (from the repo root):
  Blender -b --factory-startup --python-exit-code 1 -P docs/checkpoints/farm_animals/still.py -- <piece> <out dir> [lod]"""
import math
import pathlib
import sys

import bpy
from mathutils import Vector

ROOT = pathlib.Path(__file__).resolve().parents[3]
args = sys.argv[sys.argv.index("--") + 1:]
piece, out_dir = args[0], args[1]
lod = args[2] if len(args) > 2 else "0"
stage = ROOT / "target/kit-stage" / piece

bpy.ops.wm.read_factory_settings(use_empty=True)
scn = bpy.context.scene
scn.render.engine = "BLENDER_EEVEE"
scn.render.resolution_x, scn.render.resolution_y = 1200, 900
scn.render.use_stamp_date = False
scn.render.use_stamp_render_time = False
scn.view_settings.view_transform = "AgX"
world = bpy.data.worlds.new("still")
world.color = (0.5, 0.6, 0.75)
scn.world = world


def image(channel, non_color):
    img = bpy.data.images.load(str(stage / f"{piece}.{channel}.png"))
    img.colorspace_settings.name = "Non-Color" if non_color else "sRGB"
    return img


bpy.ops.import_scene.gltf(filepath=str(stage / f"{piece}.glb"))
for o in [o for o in bpy.data.objects if o.type == "MESH" and not o.name.startswith(f"{piece}_lod{lod}")]:
    bpy.data.objects.remove(o, do_unlink=True)
body = next(o for o in bpy.data.objects if o.type == "MESH")
mat = body.material_slots[0].material
nt = mat.node_tree
bsdf = next(n for n in nt.nodes if n.type == "BSDF_PRINCIPLED")
alb = nt.nodes.new("ShaderNodeTexImage")
alb.image = image("albedo", False)
orm = nt.nodes.new("ShaderNodeTexImage")
orm.image = image("orm", True)
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
nrm = nt.nodes.new("ShaderNodeTexImage")
nrm.image = image("normal", True)
nm = nt.nodes.new("ShaderNodeNormalMap")
nt.links.new(nrm.outputs["Color"], nm.inputs["Color"])
nt.links.new(nm.outputs["Normal"], bsdf.inputs["Normal"])

bpy.ops.mesh.primitive_plane_add(size=40.0)
g = bpy.context.object
gm = bpy.data.materials.new("ground")
next(n for n in gm.node_tree.nodes if n.type == "BSDF_PRINCIPLED").inputs["Base Color"].default_value = (0.16, 0.26, 0.07, 1)
g.data.materials.append(gm)
sun = bpy.data.objects.new("sun", bpy.data.lights.new("sun", "SUN"))
sun.data.energy = 4.0
sun.rotation_euler = (math.radians(40), 0, math.radians(-40))
scn.collection.objects.link(sun)
cam = bpy.data.objects.new("cam", bpy.data.cameras.new("cam"))
scn.collection.objects.link(cam)
scn.camera = cam
cam.data.lens = 50
bb = [body.matrix_world @ Vector(c) for c in body.bound_box]
size = max(max(v[i] for v in bb) - min(v[i] for v in bb) for i in range(3))
zmax = max(v.z for v in bb)
target = Vector((0, 0, zmax * 0.5))
k = size / 2.4
# Blender -y is the animal's forward; +x its left.
for name, off in (("left", Vector((5.2, 0.0, 0.3))), ("front_three_quarter", Vector((3.4, -3.6, 1.6))),
                  ("back_three_quarter", Vector((-3.4, 3.4, 2.0)))):
    cam.location = target + off * k
    cam.rotation_euler = (target - cam.location).to_track_quat("-Z", "Y").to_euler()
    scn.render.filepath = str(pathlib.Path(out_dir) / f"{piece}-eevee-{name}.png")
    bpy.ops.render.render(write_still=True)
print("stills written", out_dir)
