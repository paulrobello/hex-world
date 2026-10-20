"""Textured headless EEVEE stills of hay_bale_round_b's LOD2 and LOD0, before and after the apron
normal fix (evidence only: Blender's view transform is not Bevy's). In engine, forced LOD2 hides most
of the apron: its six straight 60-degree chords run inside the bale's foot at mid-segment, and the
forced-LOD2 meadow covers more. Here the ground sits 0.1 m under the pivot, so the whole apron shows.
Each <dir> holds a <piece>.glb (the meshopt GLB the importer reads, with its normals as custom normals)
and its PNG maps (albedo x ORM AO, ORM roughness, the tangent normal map). Usage (from the repo root):
  Blender -b --factory-startup --python-exit-code 1 -P docs/checkpoints/hay-bale-b-apron/still.py -- <out dir> <before dir> <after dir>"""
import math
import pathlib
import sys

import bpy
from mathutils import Vector

PIECE = "hay_bale_round_b"
args = sys.argv[sys.argv.index("--") + 1:]
out_dir = pathlib.Path(args[0])
SOURCES = {"before": pathlib.Path(args[1]), "after": pathlib.Path(args[2])}
VIEWS = {"high": Vector((1.5, -1.9, 2.3)), "top": Vector((0.25, -0.45, 2.9))}


def image(d, channel, non_color):
    img = bpy.data.images.load(str(d / f"{PIECE}.{channel}.png"))
    img.colorspace_settings.name = "Non-Color" if non_color else "sRGB"
    return img


def load(d, lod):
    before = set(bpy.data.objects)
    bpy.ops.import_scene.gltf(filepath=str(d / f"{PIECE}.glb"))
    new = [o for o in bpy.data.objects if o not in before]
    body = next(o for o in new if o.type == "MESH" and o.name.startswith(f"{PIECE}_{lod}"))
    for o in new:
        if o is not body and o.type == "MESH":
            bpy.data.objects.remove(o, do_unlink=True)
    nt = body.material_slots[0].material.node_tree
    bsdf = next(n for n in nt.nodes if n.type == "BSDF_PRINCIPLED")
    alb = nt.nodes.new("ShaderNodeTexImage")
    alb.image = image(d, "albedo", False)
    orm = nt.nodes.new("ShaderNodeTexImage")
    orm.image = image(d, "orm", True)
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
    nrm.image = image(d, "normal", True)
    nm = nt.nodes.new("ShaderNodeNormalMap")
    nt.links.new(nrm.outputs["Color"], nm.inputs["Color"])
    nt.links.new(nm.outputs["Normal"], bsdf.inputs["Normal"])


def setup():
    bpy.ops.wm.read_factory_settings(use_empty=True)
    scn = bpy.context.scene
    scn.render.engine = "BLENDER_EEVEE"
    scn.render.resolution_x, scn.render.resolution_y = 1000, 800
    scn.render.use_stamp_date = False
    scn.render.use_stamp_render_time = False
    scn.view_settings.view_transform = "AgX"
    world = bpy.data.worlds.new("still")
    world.color = (0.5, 0.6, 0.75)
    scn.world = world
    bpy.ops.mesh.primitive_plane_add(size=40.0, location=(0.0, 0.0, -0.1))
    g = bpy.context.object
    gm = bpy.data.materials.new("ground")
    next(n for n in gm.node_tree.nodes if n.type == "BSDF_PRINCIPLED").inputs["Base Color"].default_value = (0.16, 0.26, 0.07, 1)
    g.data.materials.append(gm)
    sun = bpy.data.objects.new("sun", bpy.data.lights.new("sun", "SUN"))
    sun.data.energy = 4.0
    sun.rotation_euler = (math.radians(45), 0, math.radians(30))
    scn.collection.objects.link(sun)
    cam = bpy.data.objects.new("cam", bpy.data.cameras.new("cam"))
    scn.collection.objects.link(cam)
    scn.camera = cam
    cam.data.lens = 50
    return scn, cam


for tag, d in SOURCES.items():
    for lod in ("lod2", "lod0"):
        scn, cam = setup()
        load(d, lod)
        for view, off in VIEWS.items():
            target = Vector((0.0, 0.0, 0.2))
            cam.location = target + off
            cam.rotation_euler = (target - cam.location).to_track_quat("-Z", "Y").to_euler()
            scn.render.filepath = str(out_dir / f"{tag}-{lod}-{view}.png")
            bpy.ops.render.render(write_still=True)
print("stills written", out_dir)
