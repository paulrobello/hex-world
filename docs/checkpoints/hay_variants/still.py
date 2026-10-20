"""Textured headless EEVEE stills of the hay variants (evidence only: Blender's view transform is not
Bevy's). Each variant's LOD0 from target/kit-stage/<piece>/<piece>.glb with its stage PNGs bound
(albedo x ORM AO, ORM roughness, the normal map) stands beside its original (the committed GLB and maps
in assets/kit/p1/<original>/) on a grass-coloured ground: hay-round-pair, hay-stack-pair, then each
variant alone from the three-quarter view and the low side. Usage (from the repo root):
  Blender -b --factory-startup --python-exit-code 1 -P docs/checkpoints/hay_variants/still.py -- <out dir>"""
import math
import pathlib
import sys

import bpy
from mathutils import Vector

ROOT = pathlib.Path(__file__).resolve().parents[3]
out_dir = pathlib.Path(sys.argv[sys.argv.index("--") + 1])


def source(piece):
    stage = ROOT / "target/kit-stage" / piece
    if (stage / f"{piece}.albedo.png").exists() and piece.endswith("_b"):
        return stage
    return ROOT / "assets/kit/p1" / piece


def image(d, piece, channel, non_color):
    img = bpy.data.images.load(str(d / f"{piece}.{channel}.png"))
    img.colorspace_settings.name = "Non-Color" if non_color else "sRGB"
    return img


def load(piece, at, yaw_deg):
    """LOD0 of `piece` with its maps bound, moved to `at` (Blender x, y) and turned yaw_deg about z."""
    d = source(piece)
    before = set(bpy.data.objects)
    bpy.ops.import_scene.gltf(filepath=str(d / f"{piece}.glb"))
    new = [o for o in bpy.data.objects if o not in before]
    body = next(o for o in new if o.type == "MESH" and o.name.startswith(f"{piece}_lod0"))
    for o in new:
        if o is not body and o.type == "MESH":
            bpy.data.objects.remove(o, do_unlink=True)
    mat = body.material_slots[0].material
    nt = mat.node_tree
    bsdf = next(n for n in nt.nodes if n.type == "BSDF_PRINCIPLED")
    alb = nt.nodes.new("ShaderNodeTexImage")
    alb.image = image(d, piece, "albedo", False)
    orm = nt.nodes.new("ShaderNodeTexImage")
    orm.image = image(d, piece, "orm", True)
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
    nrm.image = image(d, piece, "normal", True)
    nm = nt.nodes.new("ShaderNodeNormalMap")
    nt.links.new(nrm.outputs["Color"], nm.inputs["Color"])
    nt.links.new(nm.outputs["Normal"], bsdf.inputs["Normal"])
    root = body.parent
    holder = root if root is not None else body
    holder.rotation_mode = "XYZ"
    holder.location = (at[0], at[1], 0.0)
    holder.rotation_euler = (holder.rotation_euler[0], holder.rotation_euler[1], math.radians(yaw_deg))
    return [body] + ([root] if root else [])


def setup():
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
    bpy.ops.mesh.primitive_plane_add(size=40.0)
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


def shoot(scn, cam, target, off, name):
    cam.location = target + off
    cam.rotation_euler = (target - cam.location).to_track_quat("-Z", "Y").to_euler()
    scn.render.filepath = str(out_dir / f"{name}.png")
    bpy.ops.render.render(write_still=True)


for orig, var, pair_off in (("hay_bale_round", "hay_bale_round_b", 1.1), ("hay_bale_stack", "hay_bale_stack_b", 0.9)):
    scn, cam = setup()
    load(orig, (-pair_off, 0.0), 20.0)
    load(var, (pair_off, 0.0), 20.0)
    shoot(scn, cam, Vector((0, 0, 0.5)), Vector((2.0, -5.6, 2.4)), f"{orig.replace('hay_bale_', 'hay-')}-pair")
    scn, cam = setup()
    load(var, (0.0, 0.0), 0.0)
    for view, off in (("three_quarter", Vector((2.6, -3.4, 2.2))), ("low_side", Vector((-1.2, -4.0, 0.6)))):
        shoot(scn, cam, Vector((0, 0, 0.45)), off, f"{var}-eevee-{view}")
print("stills written", out_dir)
