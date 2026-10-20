"""Textured headless EEVEE stills of rail_road_crossing_center (evidence only: Blender's AgX view
transform is not Bevy's tonemapper; approval happens in engine). Copied from
docs/checkpoints/rail/still.py: the piece's LOD0 on a meadow_base LOD0, with road_straight_edge
stubs on its road lanes (edges 1 and 4), its slots bound the way kit_material.wesl draws them (a
tileable slot samples its world-space set box-projected at the set's tileM times its manifest
colour, a colour slot takes its manifest colour and roughness, a masked slot's diffuse is multiplied
by the piece's masks.A on UV0).

Views: `three` (three-quarter, sun side, as ref-3), `deck` (the deck and a gate about 4 m out),
`scale` (a car_sedan on the deck, beside a gate).
Usage: Blender -b --factory-startup --python-exit-code 1 -P docs/checkpoints/rail_crossing/still.py --
       <out.png> <view>"""
import json
import math
import pathlib
import sys

import bpy
from mathutils import Vector

ROOT = pathlib.Path(__file__).resolve().parents[3]
KIT = ROOT / "assets/kit"
out, view = sys.argv[sys.argv.index("--") + 1:]
man = json.loads((KIT / "manifest.json").read_text())
pieces = {p["id"]: p for p in man["pieces"]}
sets = man["materials"]
# Colour-only car slots take their paint from COLOR_0 at runtime; a plain red sedan here.
FALLBACK = {"car_sedan_body": (0.45, 0.06, 0.04), "car_sedan_glass": (0.02, 0.025, 0.03),
            "car_sedan_tyre": (0.02, 0.02, 0.02)}

bpy.ops.wm.read_factory_settings(use_empty=True)
scn = bpy.context.scene
scn.render.engine = "BLENDER_EEVEE"
scn.render.resolution_x, scn.render.resolution_y = 1600, 1000
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


def bind(mat, name, slot, masks_png):
    mat.use_backface_culling = True
    nt = mat.node_tree
    bsdf = next(n for n in nt.nodes if n.type == "BSDF_PRINCIPLED")
    base = slot.get("color") or FALLBACK.get(name, (0.8, 0.8, 0.8))
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
    if masks_png and slot.get("masks"):
        uvn = nt.nodes.new("ShaderNodeUVMap")
        uvn.uv_map = "UVMap"
        mk = nt.nodes.new("ShaderNodeTexImage")
        mk.image = image(masks_png, True)
        nt.links.new(uvn.outputs["UV"], mk.inputs["Vector"])
        mul = nt.nodes.new("ShaderNodeMix")
        mul.data_type = "RGBA"
        mul.blend_type = "MULTIPLY"
        ins = {x.identifier: x for x in mul.inputs}
        ins["Factor_Float"].default_value = 1.0
        if col is None:
            ins["A_Color"].default_value = (*base, 1.0)
        else:
            nt.links.new(col, ins["A_Color"])
        nt.links.new(mk.outputs["Alpha"], ins["B_Color"])
        col = next(x for x in mul.outputs if x.identifier == "Result_Color")
    if col is None:
        bsdf.inputs["Base Color"].default_value = (*base, 1.0)
    else:
        nt.links.new(col, bsdf.inputs["Base Color"])


def place(pid, at=(0.0, 0.0, 0.0), yaw_deg=0.0):
    before = set(bpy.data.objects)
    bpy.ops.import_scene.gltf(filepath=str(KIT / pieces[pid]["glb"]))
    new = [o for o in bpy.data.objects if o not in before]
    for o in new:
        if o.type == "MESH" and o.name != f"{pid}_lod0":
            bpy.data.objects.remove(o, do_unlink=True)
    bpy.context.view_layer.update()
    root = next(o for o in bpy.data.objects if o not in before and o.parent is None)
    root.location = Vector(at)
    root.rotation_mode = "XYZ"
    root.rotation_euler = (0.0, 0.0, math.radians(yaw_deg))
    bpy.context.view_layer.update()
    lod0 = bpy.data.objects[f"{pid}_lod0"]
    masks = pieces[pid].get("maps", {}).get("masks")
    masks_png = KIT / f"{masks}.png" if masks else None
    for s in lod0.material_slots:
        name = s.material.name.split(".")[0]
        if name in pieces[pid]["slots"]:
            bind(s.material, name, pieces[pid]["slots"][name], masks_png)
    return lod0


place("meadow_base")
place("rail_road_crossing_center")
# Road stubs on edges 1 and 4: edge k is edge 0 turned by -60 k degrees in Blender's top view.
place("road_straight_edge", yaw_deg=-60.0)
place("road_straight_edge", yaw_deg=-240.0)
if view == "scale":
    # The sedan on the deck heading along the road (toward edge 1, -60 degrees), its wheels on the
    # timber at the deck height (rail heads, 0.46 m).
    place("car_sedan", (0.0, 0.0, 0.46), yaw_deg=-60.0)

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
cam.data.lens = 35
views = {
    "three": (Vector((22.0, -26.0, 16.0)), Vector((0.0, 0.0, 0.0))),
    "deck": (Vector((5.5, -6.2, 2.2)), Vector((2.2, -1.6, 0.5))),
    "scale": (Vector((10.0, -9.0, 3.2)), Vector((0.5, -0.5, 1.0))),
}
loc, target = views[view]
cam.location = loc
cam.rotation_euler = (target - loc).to_track_quat("-Z", "Y").to_euler()
scn.render.filepath = out
bpy.ops.render.render(write_still=True)
print("still written", out)
