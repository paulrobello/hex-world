"""Textured headless EEVEE stills of the grazing sheep animal_sheep_c beside animal_sheep and the grazing
cow animal_cow_b (evidence only: Blender's view transform is not Bevy's): a LOD of
target/kit-stage/<piece>/<piece>.glb with its maps bound (albedo x ORM AO, ORM roughness, the normal
map) on a grass-coloured ground. Maps come from the committed assets/kit/p1/<piece>/ for the approved
or committed pieces (whose stage may be a skip-bake build), else from the stage. Shots: a side-on
lineup of the three with a 1.7 m figure, the grazing sheep at three-quarter front, a head close-up at
the grass, a top view (ref-3 is overhead) and LOD2 at three-quarter front.
Usage (from the repo root):
  Blender -b --factory-startup --python-exit-code 1 -P docs/checkpoints/animal_sheep_c/still.py -- <out dir>"""
import math
import pathlib
import sys

import bpy
from mathutils import Vector

ROOT = pathlib.Path(__file__).resolve().parents[3]
out_dir = pathlib.Path(sys.argv[sys.argv.index("--") + 1])
COMMITTED = ("animal_sheep", "animal_cow_b")

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


def image(path, non_color):
    img = bpy.data.images.load(str(path))
    img.colorspace_settings.name = "Non-Color" if non_color else "sRGB"
    return img


def load(piece, lod=0):
    """Import the piece's LOD from its stage GLB and bind its maps. Returns the object."""
    stage = ROOT / "target/kit-stage" / piece
    maps = ROOT / "assets/kit/p1" / piece if piece in COMMITTED else stage
    before = set(bpy.data.objects)
    bpy.ops.import_scene.gltf(filepath=str(stage / f"{piece}.glb"))
    new = [o for o in bpy.data.objects if o not in before]
    keep = next(o for o in new if o.type == "MESH" and o.name.startswith(f"{piece}_lod{lod}"))
    for o in new:
        if o is not keep and o.type == "MESH":
            bpy.data.objects.remove(o, do_unlink=True)
    keep.matrix_world = keep.matrix_world.copy()
    keep.parent = None
    mat = keep.material_slots[0].material.copy()
    keep.material_slots[0].material = mat
    nt = mat.node_tree
    bsdf = next(n for n in nt.nodes if n.type == "BSDF_PRINCIPLED")
    alb = nt.nodes.new("ShaderNodeTexImage")
    alb.image = image(maps / f"{piece}.albedo.png", False)
    orm = nt.nodes.new("ShaderNodeTexImage")
    orm.image = image(maps / f"{piece}.orm.png", True)
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
    nrm.image = image(maps / f"{piece}.normal.png", True)
    nm = nt.nodes.new("ShaderNodeNormalMap")
    nt.links.new(nrm.outputs["Color"], nm.inputs["Color"])
    nt.links.new(nm.outputs["Normal"], bsdf.inputs["Normal"])
    return keep


def shoot(name, loc, target, lens, up="Y"):
    cam.location = Vector(loc)
    cam.data.lens = lens
    cam.rotation_euler = (Vector(target) - cam.location).to_track_quat("-Z", up).to_euler()
    scn.render.filepath = str(out_dir / f"{name}.png")
    bpy.ops.render.render(write_still=True)


bpy.ops.mesh.primitive_plane_add(size=80.0)
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
bpy.ops.mesh.primitive_cylinder_add(radius=0.18, depth=1.7, location=(0, 0, 0.85))
fig = bpy.context.object
fm = bpy.data.materials.new("figure")
next(n for n in fm.node_tree.nodes if n.type == "BSDF_PRINCIPLED").inputs["Base Color"].default_value = (0.6, 0.1, 0.08, 1)
fig.data.materials.append(fm)

objs = {p: load(p) for p in ("animal_sheep", "animal_sheep_c", "animal_cow_b")}
lod2 = load("animal_sheep_c", 2)


def show(only):
    for o in list(objs.values()) + [lod2]:
        o.hide_render = o not in only


# Lineup, side-on (the animals face Blender -y; +x is their left): the cow behind, then the sheep, the
# grazing sheep nearest the camera.
objs["animal_cow_b"].location = (-1.2, -2.6, 0)
objs["animal_sheep"].location = (0.0, 0.0, 0)
objs["animal_sheep_c"].location = (0.6, 1.7, 0)
fig.location = (-0.6, 3.6, 0.85)
show(tuple(objs.values()) + (fig,))
shoot("lineup", (8.5, 0.4, 1.1), (0.0, 0.4, 0.6), 35)
fig.hide_render = True
for o in (objs["animal_sheep_c"], lod2):
    o.location = (0, 0, 0)
show((objs["animal_sheep_c"],))
shoot("animal_sheep_c-eevee-front_three_quarter", (1.55, -1.65, 0.75), (0, -0.15, 0.3), 40)
shoot("animal_sheep_c-eevee-head", (0.55, -1.35, 0.22), (0, -0.6, 0.12), 50)
objs["animal_sheep"].location = (-0.9, 0, 0)
show((objs["animal_sheep_c"], objs["animal_sheep"]))
shoot("animal_sheep_c-eevee-top", (-0.45, -0.05, 4.0), (-0.45, -0.05, 0.0), 40)
show((objs["animal_sheep_c"],))
show((lod2,))
shoot("animal_sheep_c-eevee-lod2", (1.55, -1.65, 0.75), (0, -0.15, 0.3), 40)
print("stills written", out_dir)
