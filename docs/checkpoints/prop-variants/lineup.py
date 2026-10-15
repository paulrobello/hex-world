"""Textured family lineup at true scale (headless EEVEE, evidence only: Blender's view transform is
not Bevy's): each piece's LOD0 from its runtime GLB under assets/kit/p1/, with its PNG maps bound
(albedo with the alpha cut at 0.5, normal, ORM roughness; a tileable bark slot samples the bark set
at its manifest tileUvScale and tint; a colour slot takes its manifest colour), side by side on a
grass-coloured ground.
Usage: Blender -b --factory-startup --python-exit-code 1 -P docs/checkpoints/prop-variants/lineup.py --
       <out.png> <spacing m> <camera height factor> <piece> [<piece> ...]"""
import json
import math
import pathlib
import sys

import bpy
from mathutils import Vector

ROOT = pathlib.Path(__file__).resolve().parents[3]
KIT = ROOT / "assets/kit"
argv = sys.argv[sys.argv.index("--") + 1:]
out, spacing, cam_h, pieces = argv[0], float(argv[1]), float(argv[2]), argv[3:]
manifest = {p["id"]: p for p in json.loads((KIT / "manifest.json").read_text())["pieces"]}
sets = json.loads((KIT / "manifest.json").read_text())["materials"]

bpy.ops.wm.read_factory_settings(use_empty=True)
scn = bpy.context.scene
scn.render.engine = "BLENDER_EEVEE"
scn.render.resolution_x, scn.render.resolution_y = 1600, 900
scn.render.use_stamp_date = False
scn.render.use_stamp_render_time = False
scn.view_settings.view_transform = "AgX"
world = bpy.data.worlds.new("lineup")
world.color = (0.55, 0.65, 0.8)
scn.world = world


def image(stem, non_color):
    img = bpy.data.images.load(str(KIT / f"{stem}.png"))
    img.colorspace_settings.name = "Non-Color" if non_color else "sRGB"
    return img


def bind(mat, piece, slot):
    s = manifest[piece]["slots"][slot]
    mat.use_nodes = True
    nt = mat.node_tree
    bsdf = next(n for n in nt.nodes if n.type == "BSDF_PRINCIPLED")
    if "color" in s and "tileable" not in s and "unique" not in s:
        bsdf.inputs["Base Color"].default_value = (*s["color"], 1.0)
        bsdf.inputs["Roughness"].default_value = s.get("roughness", 0.8)
        return
    uvn = nt.nodes.new("ShaderNodeUVMap")
    uvn.uv_map = "UVMap"
    vec = uvn.outputs["UV"]
    if "tileable" in s:
        maps = sets[s["tileable"]]["maps"]
        mapping = nt.nodes.new("ShaderNodeMapping")
        k = s.get("tileUvScale", 1.0)
        mapping.inputs["Scale"].default_value = (k, k, 1.0)
        nt.links.new(vec, mapping.inputs["Vector"])
        vec = mapping.outputs["Vector"]
    else:
        maps = manifest[piece]["maps"]
    alb = nt.nodes.new("ShaderNodeTexImage")
    alb.image = image(maps["albedo"], False)
    nt.links.new(vec, alb.inputs["Vector"])
    col = alb.outputs["Color"]
    if "color" in s:
        mix = nt.nodes.new("ShaderNodeMix")
        mix.data_type = "RGBA"
        mix.blend_type = "MULTIPLY"
        ins = {x.identifier: x for x in mix.inputs}
        ins["Factor_Float"].default_value = 1.0
        nt.links.new(col, ins["A_Color"])
        ins["B_Color"].default_value = (*s["color"], 1.0)
        col = next(x for x in mix.outputs if x.identifier == "Result_Color")
    nt.links.new(col, bsdf.inputs["Base Color"])
    if "alphaCutoff" in s:
        gt = nt.nodes.new("ShaderNodeMath")
        gt.operation = "GREATER_THAN"
        gt.inputs[1].default_value = s["alphaCutoff"]
        nt.links.new(alb.outputs["Alpha"], gt.inputs[0])
        nt.links.new(gt.outputs[0], bsdf.inputs["Alpha"])
    orm = nt.nodes.new("ShaderNodeTexImage")
    orm.image = image(maps["orm"], True)
    nt.links.new(vec, orm.inputs["Vector"])
    sep = nt.nodes.new("ShaderNodeSeparateColor")
    nt.links.new(orm.outputs["Color"], sep.inputs["Color"])
    nt.links.new(sep.outputs["Green"], bsdf.inputs["Roughness"])
    nrm = nt.nodes.new("ShaderNodeTexImage")
    nrm.image = image(maps["normal"], True)
    nt.links.new(vec, nrm.inputs["Vector"])
    nm = nt.nodes.new("ShaderNodeNormalMap")
    nt.links.new(nrm.outputs["Color"], nm.inputs["Color"])
    nt.links.new(nm.outputs["Normal"], bsdf.inputs["Normal"])


xmax, zmax = 0.0, 0.0
for i, pid in enumerate(pieces):
    glb = KIT / manifest[pid]["glb"]
    before = set(bpy.data.objects)
    bpy.ops.import_scene.gltf(filepath=str(glb))
    new = [o for o in bpy.data.objects if o not in before]
    for o in new:
        if o.type == "MESH" and not o.name.startswith(f"{pid}_lod0"):
            bpy.data.objects.remove(o, do_unlink=True)
    bpy.context.view_layer.update()
    for o in [o for o in bpy.data.objects if o not in before and o.type == "MESH"]:
        for slot in o.material_slots:
            if slot.material and slot.material.name.split(".")[0] in manifest[pid]["slots"]:
                bind(slot.material, pid, slot.material.name.split(".")[0])
        o.location.x += i * spacing
        zmax = max(zmax, max((o.matrix_world @ Vector(c)).z for c in o.bound_box))
    xmax = i * spacing
bpy.ops.mesh.primitive_plane_add(size=max(60.0, 4 * spacing * len(pieces)), location=(xmax / 2, 0, 0))
g = bpy.context.object
gm = bpy.data.materials.new("ground")
gm.use_nodes = True
next(n for n in gm.node_tree.nodes if n.type == "BSDF_PRINCIPLED").inputs["Base Color"].default_value = (0.16, 0.26, 0.07, 1)
g.data.materials.append(gm)
sun = bpy.data.objects.new("sun", bpy.data.lights.new("sun", "SUN"))
sun.data.energy = 4.0
sun.rotation_euler = (math.radians(50), 0, math.radians(35))
scn.collection.objects.link(sun)
cam = bpy.data.objects.new("cam", bpy.data.cameras.new("cam"))
scn.collection.objects.link(cam)
scn.camera = cam
span = max(xmax + spacing, zmax * 1.8)
target = Vector((xmax / 2, 0, zmax * 0.42))
cam.location = target + Vector((0, -span * 1.05, zmax * cam_h))
cam.rotation_euler = (target - cam.location).to_track_quat("-Z", "Y").to_euler()
cam.data.lens = 40
scn.render.filepath = out
bpy.ops.render.render(write_still=True)
print("lineup written", out)
