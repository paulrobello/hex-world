"""Textured headless EEVEE stills of the beach dressing scatter props (evidence only: Blender's view
transform is not Bevy's). LOD0 of every beach_driftwood* and beach_dunegrass* piece from
assets/kit/p1 with its maps bound (albedo, with alpha clip 0.5 on the marram cards, ORM roughness and
AO, the normal map); the mound's world-space sand slot takes a flat sand colour near the beach
tint. A lineup at true scale beside a 1.7 m figure, then a three-quarter still of each piece.
Usage (from the repo root):
  Blender -b --factory-startup --python-exit-code 1 -P docs/checkpoints/beach-dressing/still.py -- <out dir>"""
import json
import math
import pathlib
import sys

import bpy
from mathutils import Vector

ROOT = pathlib.Path(__file__).resolve().parents[3]
out_dir = pathlib.Path(sys.argv[sys.argv.index("--") + 1])
PIECES = ("beach_driftwood", "beach_driftwood_b", "beach_driftwood_c", "beach_dunegrass",
          "beach_dunegrass_b", "beach_dunegrass_c")
MANIFEST = {p["id"]: p for p in json.loads((ROOT / "assets/kit/manifest.json").read_text())["pieces"]}
SAND = (0.74, 0.66, 0.5, 1)

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
        spec = MANIFEST[piece]["slots"].get(slot.material.name, {})
        if spec.get("unique"):
            bind(slot.material, src, piece, "alphaCutoff" in spec)
        elif "tileable" in spec:
            principled(slot.material).inputs["Base Color"].default_value = SAND
            principled(slot.material).inputs["Roughness"].default_value = 0.95
    return keep


def shoot(name, loc, target, lens):
    cam.location = Vector(loc)
    cam.data.lens = lens
    cam.rotation_euler = (Vector(target) - cam.location).to_track_quat("-Z", "Y").to_euler()
    scn.render.filepath = str(out_dir / f"{name}.png")
    bpy.ops.render.render(write_still=True)


bpy.ops.mesh.primitive_plane_add(size=60.0, location=(0, 0, 0))
ground = bpy.context.object
gm = bpy.data.materials.new("sand")
principled(gm).inputs["Base Color"].default_value = SAND
principled(gm).inputs["Roughness"].default_value = 0.95
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


spots = {"beach_driftwood": (-2.2, 1.6), "beach_driftwood_b": (0.9, 1.9), "beach_driftwood_c": (3.6, 1.4),
         "beach_dunegrass": (-2.6, -0.6), "beach_dunegrass_b": (-0.4, -0.9), "beach_dunegrass_c": (1.9, -0.7)}
for p, o in objs.items():
    o.location = (*spots[p], 0.0)
fig.location = (4.3, -0.9, 0.85)
show((*PIECES, "fig"))
shoot("lineup", (0.6, -8.2, 3.4), (0.6, 0.4, 0.3), 32)
for p, o in objs.items():
    o.location = (0, 0, 0)
    show((p,))
    long = p.startswith("beach_driftwood")
    d = 3.6 if long else 2.1
    shoot(f"{p}-eevee", (d * 0.7, -d, d * 0.55 if long else 1.0), (0, 0, 0.12 if long else 0.45), 40)
print("stills written", out_dir, PIECES)
