"""Textured headless EEVEE stills of river_rail_bridge_center (evidence only: Blender's AgX view
transform is not Bevy's tonemapper; approval happens in engine). Adapted from
docs/checkpoints/rail_crossing/still.py: the piece's LOD0 on a river_straight_base LOD0 with
river_edge stubs on the river lanes (edges 0 and 3), slots bound the way kit_material.wesl draws them
(a tileable slot samples its world-space set box-projected at the set's tileM times its manifest
colour, a colour slot takes its manifest colour and roughness, a masked slot's diffuse is multiplied
by the piece's masks.A on UV0). A flat translucent plane at the contract's water level (-1.4 m,
|y| <= 4 m) stands in for the engine's water, which the GLBs do not carry.

Views: `three` (three-quarter, sun side), `arch` (the arch from the channel, about 5 m out),
`scale` (the engine and a carriage on the deck).
Usage: Blender -b --factory-startup --python-exit-code 1 -P docs/checkpoints/rail_bridge/still.py --
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
# The train's body colour comes from COLOR_0 and a runtime tint; plain stand-ins here.
FALLBACK = {"train_engine_body": (0.03, 0.12, 0.06), "train_engine_glass": (0.02, 0.025, 0.03),
            "train_engine_tyre": (0.03, 0.03, 0.03), "train_carriage_body": (0.25, 0.05, 0.03),
            "train_carriage_glass": (0.02, 0.025, 0.03), "train_carriage_tyre": (0.03, 0.03, 0.03)}

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


def mix_node(nt):
    m = nt.nodes.new("ShaderNodeMix")
    m.data_type = "RGBA"
    m.blend_type = "MULTIPLY"
    ins = {x.identifier: x for x in m.inputs}
    ins["Factor_Float"].default_value = 1.0
    return m, ins, next(x for x in m.outputs if x.identifier == "Result_Color")


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
        _, ins, col = mix_node(nt)
        nt.links.new(alb.outputs["Color"], ins["A_Color"])
        ins["B_Color"].default_value = (*[min(c, 1.0) for c in slot.get("color", (1, 1, 1))], 1.0)
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
        _, ins, out_col = mix_node(nt)
        if col is None:
            ins["A_Color"].default_value = (*base, 1.0)
        else:
            nt.links.new(col, ins["A_Color"])
        nt.links.new(mk.outputs["Alpha"], ins["B_Color"])
        col = out_col
    if col is None:
        bsdf.inputs["Base Color"].default_value = (*base, 1.0)
    else:
        nt.links.new(col, bsdf.inputs["Base Color"])


def place(pid, at=(0.0, 0.0, 0.0), yaw_deg=0.0):
    before = set(bpy.data.objects)
    bpy.ops.import_scene.gltf(filepath=str(KIT / pieces[pid]["glb"]))
    new = [o for o in bpy.data.objects if o not in before]
    for o in new:
        if o.type == "MESH" and o.name.split(".")[0] != f"{pid}_lod0":
            bpy.data.objects.remove(o, do_unlink=True)
    bpy.context.view_layer.update()
    root = next(o for o in bpy.data.objects if o not in before and o.parent is None)
    root.location = Vector(at)
    root.rotation_mode = "XYZ"
    root.rotation_euler = (0.0, 0.0, math.radians(yaw_deg))
    bpy.context.view_layer.update()
    lod0 = next(o for o in bpy.data.objects if o not in before and o.name.split(".")[0] == f"{pid}_lod0")
    masks = pieces[pid].get("maps", {}).get("masks")
    masks_png = KIT / f"{masks}.png" if masks else None
    for s in lod0.material_slots:
        name = s.material.name.split(".")[0]
        if name in pieces[pid]["slots"]:
            bind(s.material, name, pieces[pid]["slots"][name], masks_png)
    return lod0


place("river_straight_base")
place("river_rail_bridge_center")
# River stubs on edges 0 and 3: edge k is edge 0 turned by -60 k degrees in Blender's top view.
place("river_edge")
place("river_edge", yaw_deg=-180.0)
# The contract's water level over the channel (the engine draws water from the profiles).
bpy.ops.mesh.primitive_plane_add(size=1.0, location=(0.0, 0.0, -1.4))
water = bpy.context.active_object
water.scale = (2 * 13.856, 8.0, 1.0)
wm = bpy.data.materials.new("water")
wb = next(n for n in wm.node_tree.nodes if n.type == "BSDF_PRINCIPLED")
wb.inputs["Base Color"].default_value = (0.05, 0.22, 0.24, 1.0)
wb.inputs["Roughness"].default_value = 0.08
wb.inputs["Alpha"].default_value = 0.55
water.data.materials.append(wm)
if view == "scale":
    # The track runs along Blender (0.5, -0.866) (yaw -60); the carriage 8.04 m behind the engine.
    place("train_engine", (0.0, 0.0, 0.46), yaw_deg=-60.0)
    place("train_carriage", (-4.02, 6.963, 0.46), yaw_deg=-60.0)

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
    "three": (Vector((24.0, -24.0, 15.0)), Vector((0.0, 0.0, -0.5))),
    "arch": (Vector((6.5, 0.6, -0.6)), Vector((0.0, 0.0, -0.9))),
    "scale": (Vector((14.0, 8.0, 3.0)), Vector((0.0, 1.0, 1.4))),
}
loc, target = views[view]
cam.location = loc
cam.rotation_euler = (target - loc).to_track_quat("-Z", "Y").to_euler()
scn.render.filepath = out
bpy.ops.render.render(write_still=True)
print("still written", out)
