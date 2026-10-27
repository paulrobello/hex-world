"""Textured headless EEVEE stills of the B1 granite set (evidence only: Blender's view transform is
not Bevy's). LOD0 of each target/kit-stage/<id>/<id>.glb with its maps bound (albedo times ORM AO,
ORM roughness, the normal map) on a meadow-green ground, in a lineup with the committed
rock_small and rock_small_b (assets/kit/p1) and a 1.7 m figure; then a three-quarter view of each.
Usage (from the repo root):
  Blender -b --factory-startup --python-exit-code 1 -P docs/checkpoints/boulders/still.py -- <out dir>"""
import math
import pathlib
import sys

import bpy
from mathutils import Vector

ROOT = pathlib.Path(__file__).resolve().parents[3]
out_dir = pathlib.Path(sys.argv[sys.argv.index("--") + 1])
NEW = ("boulder_granite", "boulder_granite_b", "boulder_granite_c", "boulder_granite_d", "outcrop_granite")
OLD = ("rock_small", "rock_small_b")

bpy.ops.wm.read_factory_settings(use_empty=True)
scn = bpy.context.scene
scn.render.engine = "BLENDER_EEVEE"
scn.render.resolution_x, scn.render.resolution_y = 1400, 800
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


def load(piece):
    src = ROOT / "assets/kit/p1" / piece if piece in OLD else ROOT / "target/kit-stage" / piece
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
        bind(slot.material, src, piece)
    return keep


def shoot(name, loc, target, lens):
    cam.location = Vector(loc)
    cam.data.lens = lens
    cam.rotation_euler = (Vector(target) - cam.location).to_track_quat("-Z", "Y").to_euler()
    scn.render.filepath = str(out_dir / f"{name}.png")
    bpy.ops.render.render(write_still=True)


bpy.ops.mesh.primitive_plane_add(size=1.0, location=(0, 0, 0))
ground = bpy.context.object
ground.scale = (120, 120, 1)
gm = bpy.data.materials.new("meadow")
principled(gm).inputs["Base Color"].default_value = (0.09, 0.17, 0.04, 1)
principled(gm).inputs["Roughness"].default_value = 0.9
ground.data.materials.append(gm)
sun = bpy.data.objects.new("sun", bpy.data.lights.new("sun", "SUN"))
sun.data.energy = 4.0
sun.rotation_euler = (math.radians(50), 0, math.radians(35))
scn.collection.objects.link(sun)
cam = bpy.data.objects.new("cam", bpy.data.cameras.new("cam"))
scn.collection.objects.link(cam)
scn.camera = cam
bpy.ops.mesh.primitive_cylinder_add(radius=0.18, depth=1.7, location=(0, 0, 0.85))
fig = bpy.context.object
fm = bpy.data.materials.new("figure")
principled(fm).inputs["Base Color"].default_value = (0.6, 0.1, 0.08, 1)
fig.data.materials.append(fm)

objs = {p: load(p) for p in (*OLD, *NEW)}


def show(only):
    for p, o in objs.items():
        o.hide_render = p not in only
    fig.hide_render = "fig" not in only


xs = {"rock_small": -12.2, "rock_small_b": -10.4, "boulder_granite": -7.9, "boulder_granite_b": -5.0,
      "boulder_granite_c": -1.6, "boulder_granite_d": 2.8, "outcrop_granite": 10.0}
for p, o in objs.items():
    o.location = (xs[p], 0, 0)
fig.location = (-14.0, -0.5, 0.85)
show((*OLD, *NEW, "fig"))
shoot("boulders-lineup", (-1.0, -24.0, 5.0), (-1.0, 0.0, 0.8), 30)
for p in NEW:
    o = objs[p]
    o.location = (0, 0, 0)
    fig.location = (o.dimensions.x * 0.5 + 0.9, -o.dimensions.y * 0.5 - 0.4, 0.85)
    show((p, "fig"))
    r = max(o.dimensions.x, o.dimensions.y)
    shoot(f"{p}-eevee-three_quarter", (1.1 * r, -1.5 * r, 0.8 * r), (0, 0, 0.35 * o.dimensions.z), 40)
print("stills written", out_dir)
