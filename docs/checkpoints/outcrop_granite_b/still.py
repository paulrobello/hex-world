"""Textured headless EEVEE stills of one outcrop (outcrop_granite or outcrop_granite_b) from fixed views,
so the two variants compare one to one on the review sheet (`sheet.py`). Evidence only: Blender's
view transform is not Bevy's. LOD0 of <src>/<piece>.glb with its maps from <maps> bound (albedo times
ORM AO, ORM roughness, the normal map) on a meadow-green ground, with a 1.7 m figure. The views are
outcrop_granite's redo views (docs/checkpoints/outcrop_granite/redo_still.py).
Usage (from the repo root):
  Blender -b --factory-startup --python-exit-code 1 -P docs/checkpoints/outcrop_granite_b/still.py -- <piece> <src> <maps> <out dir>
<src> is a staged, uncompressed GLB folder (`target/kit-stage/<piece>` after `make kit-build`, or a
`HEXW_SKIP_BAKE=1` run of the generator into a scratch folder, which has the same LOD0 and UVs);
<maps> holds the piece's PNG maps (assets/kit/p1/<piece>)."""
import math
import pathlib
import sys

import bpy
from mathutils import Vector

args = sys.argv[sys.argv.index("--") + 1:]
piece, src, maps, out_dir = args[0], pathlib.Path(args[1]), pathlib.Path(args[2]), pathlib.Path(args[3])
out_dir.mkdir(parents=True, exist_ok=True)

bpy.ops.wm.read_factory_settings(use_empty=True)
scn = bpy.context.scene
scn.render.engine = "BLENDER_EEVEE"
scn.render.resolution_x, scn.render.resolution_y = 1200, 750
scn.render.use_stamp_date = False
scn.render.use_stamp_render_time = False
scn.view_settings.view_transform = "AgX"
world = bpy.data.worlds.new("still")
world.color = (0.45, 0.55, 0.7)
scn.world = world


def principled(mat):
    return next(n for n in mat.node_tree.nodes if n.type == "BSDF_PRINCIPLED")


def image(path, non_color):
    img = bpy.data.images.load(str(path))
    img.colorspace_settings.name = "Non-Color" if non_color else "sRGB"
    return img


def bind(mat):
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


before = set(bpy.data.objects)
bpy.ops.import_scene.gltf(filepath=str(src / f"{piece}.glb"))
new = [o for o in bpy.data.objects if o not in before]
rock = next(o for o in new if o.type == "MESH" and o.name.startswith(f"{piece}_lod0"))
for o in new:
    if o is not rock and o.type == "MESH":
        bpy.data.objects.remove(o, do_unlink=True)
rock.matrix_world = rock.matrix_world.copy()
rock.parent = None
for slot in rock.material_slots:
    bind(slot.material)

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
bpy.ops.mesh.primitive_cylinder_add(radius=0.18, depth=1.7, location=(4.9, -2.9, 0.85))
fig = bpy.context.object
fm = bpy.data.materials.new("figure")
principled(fm).inputs["Base Color"].default_value = (0.6, 0.1, 0.08, 1)
fig.data.materials.append(fm)

VIEWS = {
    "three_quarter": ((8.8, -12.0, 6.4), (0.0, 0.0, 0.42), 40),
    "play": ((9.0, -20.0, 22.0), (0.0, 0.0, 0.4), 45),
    "top": ((0.0, -0.01, 24.0), (0.0, 0.0, 0.0), 40),
    "close_sun": ((4.6, -5.2, 3.0), (0.9, -0.3, 0.6), 35),
    "close_shade": ((-4.6, 5.2, 3.0), (-0.9, 0.3, 0.6), 35),
    "eye": ((1.0, -7.5, 1.7), (0.0, 0.0, 0.6), 30),
}
for name, (loc, target, lens) in VIEWS.items():
    cam.location = Vector(loc)
    cam.data.lens = lens
    cam.rotation_euler = (Vector(target) - cam.location).to_track_quat("-Z", "Y").to_euler()
    scn.render.filepath = str(out_dir / f"{piece}-{name}.png")
    bpy.ops.render.render(write_still=True)
print("stills written", out_dir, piece)
