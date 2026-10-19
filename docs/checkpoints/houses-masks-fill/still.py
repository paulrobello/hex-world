"""Textured headless EEVEE still of one house-family piece (evidence only: Blender's AgX view transform
is not Bevy's tonemapper). Adapted from docs/checkpoints/barn_red/still.py: LOD0 is imported from the
GLB with its slots bound as kit_material.wesl draws them: a tileable slot samples the world-space
`plaster` set (box-projected at the set's tileM) times its manifest colour, a colour slot takes its
manifest colour and roughness, and every slot's base colour is multiplied by masks.A (baked AO, on UV0),
so a triangle on an unbaked texel (A = 0) reads black here as it does in engine. Before/after pairs use
the same camera and only swap the GLB and masks PNG.

Views: `front` (three-quarter on the door side, +x), `back` (three-quarter on the back, -x).
Usage: Blender -b --factory-startup --python-exit-code 1 -P docs/checkpoints/houses-masks-fill/still.py --
       <out.png> <view> <piece id> <glb> <masks png> <manifest json>"""
import json
import math
import pathlib
import sys

import bpy
from mathutils import Vector

ROOT = pathlib.Path(__file__).resolve().parents[3]
KIT = ROOT / "assets/kit"
argv = sys.argv[sys.argv.index("--") + 1:]
out, view, pid, glb, masks_png, manifest_path = argv
man = json.loads(pathlib.Path(manifest_path).read_text())
piece = next(p for p in man["pieces"] if p["id"] == pid)
sets = man["materials"]

bpy.ops.wm.read_factory_settings(use_empty=True)
scn = bpy.context.scene
scn.render.engine = "BLENDER_EEVEE"
scn.render.resolution_x, scn.render.resolution_y = 1200, 900
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


masks_img = image(masks_png, True)
# Nearest, as the gate samples: the defect is a per-triangle texel lookup.
masks_interp = "Closest"


def bind(mat, slot):
    mat.use_backface_culling = True
    nt = mat.node_tree
    bsdf = next(n for n in nt.nodes if n.type == "BSDF_PRINCIPLED")
    col = None
    if "tileable" in slot:
        st = sets[slot["tileable"]]
        tc = nt.nodes.new("ShaderNodeTexCoord")
        mp = nt.nodes.new("ShaderNodeMapping")
        k = 1.0 / st["tileM"]
        mp.inputs["Scale"].default_value = (k, k, k)
        nt.links.new(tc.outputs["Object"], mp.inputs["Vector"])
        alb = nt.nodes.new("ShaderNodeTexImage")
        alb.image = image(KIT / f"{st['maps']['albedo']}.png", False)
        alb.projection = "BOX"
        alb.projection_blend = 0.15
        nt.links.new(mp.outputs["Vector"], alb.inputs["Vector"])
        mix = nt.nodes.new("ShaderNodeMix")
        mix.data_type = "RGBA"
        mix.blend_type = "MULTIPLY"
        ins = {x.identifier: x for x in mix.inputs}
        ins["Factor_Float"].default_value = 1.0
        nt.links.new(alb.outputs["Color"], ins["A_Color"])
        ins["B_Color"].default_value = (*slot.get("color", (1, 1, 1)), 1.0)
        col = next(x for x in mix.outputs if x.identifier == "Result_Color")
        orm = nt.nodes.new("ShaderNodeTexImage")
        orm.image = image(KIT / f"{st['maps']['orm']}.png", True)
        orm.projection = "BOX"
        orm.projection_blend = 0.15
        nt.links.new(mp.outputs["Vector"], orm.inputs["Vector"])
        sep = nt.nodes.new("ShaderNodeSeparateColor")
        nt.links.new(orm.outputs["Color"], sep.inputs["Color"])
        nt.links.new(sep.outputs["Green"], bsdf.inputs["Roughness"])
    else:
        bsdf.inputs["Roughness"].default_value = slot.get("roughness", 0.8)
    uvn = nt.nodes.new("ShaderNodeUVMap")
    uvn.uv_map = "UVMap"
    mk = nt.nodes.new("ShaderNodeTexImage")
    mk.image = masks_img
    mk.interpolation = masks_interp
    nt.links.new(uvn.outputs["UV"], mk.inputs["Vector"])
    mul = nt.nodes.new("ShaderNodeMix")
    mul.data_type = "RGBA"
    mul.blend_type = "MULTIPLY"
    ins = {x.identifier: x for x in mul.inputs}
    ins["Factor_Float"].default_value = 1.0
    if col is None:
        ins["A_Color"].default_value = (*slot["color"], 1.0)
    else:
        nt.links.new(col, ins["A_Color"])
    nt.links.new(mk.outputs["Alpha"], ins["B_Color"])
    nt.links.new(next(x for x in mul.outputs if x.identifier == "Result_Color"), bsdf.inputs["Base Color"])


bpy.ops.import_scene.gltf(filepath=str(glb))
for o in list(bpy.data.objects):
    if o.type == "MESH" and o.name != f"{pid}_lod0":
        bpy.data.objects.remove(o, do_unlink=True)
bpy.context.view_layer.update()
lod0 = bpy.data.objects[f"{pid}_lod0"]
for s in lod0.material_slots:
    name = s.material.name.split(".")[0]
    if name in piece["slots"]:
        bind(s.material, piece["slots"][name])

bpy.ops.mesh.primitive_plane_add(size=80.0, location=(0, 0, -0.02))
g = bpy.context.object
gm = bpy.data.materials.new("ground")
next(n for n in gm.node_tree.nodes if n.type == "BSDF_PRINCIPLED").inputs["Base Color"].default_value = (0.13, 0.22, 0.06, 1)
g.data.materials.append(gm)
# The kit sun (lighting.rs KIT_SUN_DIR, runtime (0.553, 0.742, 0.379)) in Blender's frame.
sun_dir = Vector((0.553, -0.379, 0.742)).normalized()
sun = bpy.data.objects.new("sun", bpy.data.lights.new("sun", "SUN"))
sun.data.energy = 4.5
sun.data.angle = math.radians(2.0)
sun.rotation_euler = (-sun_dir).to_track_quat("-Z", "Y").to_euler()
scn.collection.objects.link(sun)
cam = bpy.data.objects.new("cam", bpy.data.cameras.new("cam"))
scn.collection.objects.link(cam)
scn.camera = cam
cam.data.lens = 40
h = max(v.co.z for v in lod0.data.vertices)
target = Vector((0.0, 0.0, h * 0.45))
views = {"front": Vector((11.0, -8.0, 5.5)), "back": Vector((-10.5, 8.5, 5.0))}
loc = views[view] * max(1.0, h / 7.0)
cam.location = loc
cam.rotation_euler = (target - loc).to_track_quat("-Z", "Y").to_euler()
scn.render.filepath = out
bpy.ops.render.render(write_still=True)
print("still written", out)
