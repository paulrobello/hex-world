"""Oak trunk-join stills: headless EEVEE renders of a tree_hero_oak GLB's LOD0 with the bark set bound
the way the runtime binds it (UV0 x tileUvScale for the set's albedo, normal and ORM; raw UV0 for the
piece's masks, whose A is the bark AO), the canopy and leaves flat green, on a flat ground, lit by a
sun from +x +z (runtime frame, as the kit scenario's KIT_SUN_DIR). Cameras mirror the in-game close-ups
(docs/checkpoints/oak-trunk/oak-trunk.json, about 3.7 m from the trunk) plus a flat-shaded clay view
of the join. Look-dev only: Blender's view transform is not Bevy's tonemapper.
Usage: Blender -b --factory-startup --python-exit-code 1 -P render_join.py -- \
  <glb> <masks.png> <tileUvScale> <bark set dir> <out prefix>"""
import math
import sys

import bpy
from mathutils import Vector

glb, masks, k_tile, bark_dir, prefix = sys.argv[sys.argv.index("--") + 1:][:5]
k_tile = float(k_tile)
bpy.ops.wm.read_factory_settings(use_empty=True)
scn = bpy.context.scene
bpy.ops.import_scene.gltf(filepath=glb)
for o in list(scn.objects):
    if o.type == "MESH" and not o.name.endswith("lod0"):
        bpy.data.objects.remove(o, do_unlink=True)
lod0 = next(o for o in scn.objects if o.type == "MESH")


def image(path, non_color):
    im = bpy.data.images.load(path)
    im.colorspace_settings.name = "Non-Color" if non_color else "sRGB"
    return im


def bark_material(name, clay=False):
    m = bpy.data.materials.new(name)
    nt = m.node_tree
    bsdf = nt.nodes["Principled BSDF"]
    if clay:
        bsdf.inputs["Base Color"].default_value = (0.5, 0.5, 0.5, 1)
        bsdf.inputs["Roughness"].default_value = 0.8
        return m
    uv = nt.nodes.new("ShaderNodeUVMap")
    uv.uv_map = "UVMap"
    mp = nt.nodes.new("ShaderNodeMapping")
    mp.inputs["Scale"].default_value = (k_tile, k_tile, 1)
    nt.links.new(uv.outputs["UV"], mp.inputs["Vector"])
    tex = {}
    for ch in ("albedo", "normal", "orm"):
        t = nt.nodes.new("ShaderNodeTexImage")
        t.image = image(f"{bark_dir}/bark.{ch}.png", ch != "albedo")
        nt.links.new(mp.outputs["Vector"], t.inputs["Vector"])
        tex[ch] = t
    mk = nt.nodes.new("ShaderNodeTexImage")
    mk.image = image(masks, True)
    nt.links.new(uv.outputs["UV"], mk.inputs["Vector"])
    ao = nt.nodes.new("ShaderNodeMath")
    ao.operation = "MULTIPLY"
    orm = nt.nodes.new("ShaderNodeSeparateColor")
    nt.links.new(tex["orm"].outputs["Color"], orm.inputs["Color"])
    nt.links.new(orm.outputs["Red"], ao.inputs[0])
    nt.links.new(mk.outputs["Alpha"], ao.inputs[1])
    mix = nt.nodes.new("ShaderNodeMix")
    mix.data_type = "RGBA"
    mix.blend_type = "MULTIPLY"
    mix.inputs["Factor"].default_value = 1.0
    nt.links.new(tex["albedo"].outputs["Color"], mix.inputs["A"])
    nt.links.new(ao.outputs[0], mix.inputs["B"])
    nt.links.new(mix.outputs["Result"], bsdf.inputs["Base Color"])
    nt.links.new(orm.outputs["Green"], bsdf.inputs["Roughness"])
    nm = nt.nodes.new("ShaderNodeNormalMap")
    nm.uv_map = "UVMap"
    nt.links.new(tex["normal"].outputs["Color"], nm.inputs["Color"])
    nt.links.new(nm.outputs["Normal"], bsdf.inputs["Normal"])
    return m


def flat(name, rgb):
    m = bpy.data.materials.new(name)
    m.node_tree.nodes["Principled BSDF"].inputs["Base Color"].default_value = (*rgb, 1)
    return m


textured, clay = bark_material("join_bark"), bark_material("join_clay", clay=True)
green = flat("join_green", (0.05, 0.12, 0.03))
bark_slots = [i for i, s in enumerate(lod0.material_slots) if s.material and s.material.name.endswith("_bark")]
for i, s in enumerate(lod0.material_slots):
    if i not in bark_slots:
        s.material = green
bpy.ops.mesh.primitive_plane_add(size=40, location=(0, 0, 0))
bpy.context.object.data.materials.append(flat("join_ground", (0.16, 0.13, 0.08)))
sun = bpy.data.objects.new("sun", bpy.data.lights.new("sun", "SUN"))
sun.data.energy = 4.0
scn.collection.objects.link(sun)
sun.rotation_euler = (Vector((1.0, -1.0, 1.3)).to_track_quat("Z", "Y")).to_euler()
scn.world = bpy.data.worlds.new("w")
scn.world.color = (0.35, 0.42, 0.5)
cam = bpy.data.objects.new("cam", bpy.data.cameras.new("cam"))
cam.data.lens = 24
scn.collection.objects.link(cam)
scn.camera = cam
scn.render.engine = "BLENDER_EEVEE"
scn.render.resolution_x, scn.render.resolution_y = 1280, 800
scn.render.use_stamp_date = scn.render.use_stamp_render_time = False
scn.render.image_settings.file_format = "PNG"
# Runtime close-up offsets (x, y, z) from the trunk -> Blender (x, -z, y).
views = {"sun": ((3.0, -2.2, 1.1), (0, 0, 1.6)), "shade": ((-3.14, 2.2, 1.1), (0, 0, 1.6)),
         "low": ((2.6, -3.4, 0.5), (0, 0, 1.0))}
for mode, mat in (("tex", textured), ("clay", clay)):
    for i in bark_slots:
        lod0.material_slots[i].material = mat
    lod0.visible_shadow = mode != "clay"  # clay: no canopy shadow, so the join's form shows
    for view, (pos, look) in views.items():
        if mode == "clay" and view != "sun":
            continue
        cam.location = pos
        cam.rotation_euler = (Vector(look) - Vector(pos)).to_track_quat("-Z", "Y").to_euler()
        scn.render.filepath = f"{prefix}-{mode}-{view}.png"
        bpy.ops.render.render(write_still=True)
print("rendered", prefix, math.pi)
