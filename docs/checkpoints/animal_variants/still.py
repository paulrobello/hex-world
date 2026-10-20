"""Textured headless EEVEE stills of the farm animal variants beside their originals (evidence only:
Blender's view transform is not Bevy's): a LOD of target/kit-stage/<piece>/<piece>.glb with its maps
bound (albedo x ORM AO, ORM roughness, the normal map) on a grass-coloured ground. Maps come from the
committed assets/kit/p1/<piece>/ for the originals (whose stage may be a skip-bake build), else from
the stage. Lineups: animal_cow beside animal_cow_b, animal_sheep beside animal_sheep_b, side-on as in
the references, with a 1.7 m figure; then each variant alone at three-quarter front and at LOD2.
Usage (from the repo root):
  Blender -b --factory-startup --python-exit-code 1 -P docs/checkpoints/animal_variants/still.py -- <out dir>"""
import math
import pathlib
import sys

import bpy
from mathutils import Vector

ROOT = pathlib.Path(__file__).resolve().parents[3]
out_dir = pathlib.Path(sys.argv[sys.argv.index("--") + 1])
ORIGINALS = ("animal_cow", "animal_sheep")

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
    maps = ROOT / "assets/kit/p1" / piece if piece in ORIGINALS else stage
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


def shoot(name, loc, target, lens):
    cam.location = Vector(loc)
    cam.data.lens = lens
    cam.rotation_euler = (Vector(target) - cam.location).to_track_quat("-Z", "Y").to_euler()
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

objs = {p: load(p) for p in ("animal_cow", "animal_cow_b", "animal_sheep", "animal_sheep_b")}
lod2 = {p: load(p, 2) for p in ("animal_cow_b", "animal_sheep_b")}


def show(only):
    for o in list(objs.values()) + list(lod2.values()):
        o.hide_render = o not in only


# Lineups, side-on (the animals face Blender -y; +x is their left), the original behind.
objs["animal_cow"].location = (0, 0.0, 0)
objs["animal_cow_b"].location = (0, 0.0, 0)
objs["animal_cow"].location.x = -1.2
objs["animal_cow_b"].location.x = 1.0
fig.location = (0.0, 2.2, 0.85)
show((objs["animal_cow"], objs["animal_cow_b"], fig))
shoot("cows-lineup", (8.5, -0.6, 1.2), (0.0, -0.2, 0.75), 35)
objs["animal_sheep"].location = (-0.7, 0.0, 0)
objs["animal_sheep_b"].location = (0.55, 0.0, 0)
fig.location = (0.0, 1.4, 0.85)
show((objs["animal_sheep"], objs["animal_sheep_b"], fig))
shoot("sheep-lineup", (5.0, -0.4, 0.75), (0.0, -0.1, 0.45), 35)
# Each variant alone: three-quarter front and a head close-up at LOD0, the three-quarter at LOD2.
HEAD_CAM = {"animal_cow_b": ((1.7, -2.6, 0.55), (0, -1.0, 0.4)),
            "animal_sheep_b": ((0.75, -1.35, 0.42), (0, -0.3, 0.3))}
for p, k in (("animal_cow_b", 1.0), ("animal_sheep_b", 0.45)):
    for o in (objs[p], lod2[p]):
        o.location = (0, 0, 0)
    show((objs[p],))
    shoot(f"{p}-eevee-front_three_quarter", (3.4 * k, -3.6 * k, 1.6 * k), (0, -0.3 * k, 0.6 * k), 40)
    shoot(f"{p}-eevee-head", *HEAD_CAM[p], 50)
    show((lod2[p],))
    shoot(f"{p}-eevee-lod2", (3.4 * k, -3.6 * k, 1.6 * k), (0, -0.3 * k, 0.6 * k), 40)
print("stills written", out_dir)
