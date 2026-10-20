"""Textured headless EEVEE stills of fence_paddock on the farm tile (evidence only: Blender's AgX view
transform is not Bevy's tonemapper; approval happens in engine). The meadow_base cap and the farm's
heroes (data/tiles/farm.ron: barn, bales, cow, sheep) are imported from the committed kit at their
tile positions, with the fence from its stage GLB at the tile centre. Slots are bound the way
kit_material.wesl draws them: a tileable slot samples its world-space set (box-projected at the set's
tileM, an approximation of `tile_frame`) times its manifest colour, a unique slot its albedo, and every
slot's diffuse is multiplied by the piece's masks.A on UV0.

Hero transform (play.rs): runtime (at.x, 0, at.z), rotation about +Y by -yaw; in Blender that is
location (x, -z, 0) and a rotation of -yaw about Z (Blender Z is runtime Y, and the frame flip keeps
the sign: a runtime turn by -yaw about +Y is a Blender turn by -yaw about +Z).

Views: `hero` (the paddock from the south-east, three-quarter, sun side), `close` (a 4 m close-up of
a corner and the gate leaf), `top` (overhead, the outline against the heroes).
Usage: Blender -b --factory-startup --python-exit-code 1 -P docs/checkpoints/fence_paddock/still.py --
       <out.png> <view> <fence glb> <fence masks png> <manifest json>"""
import json
import math
import pathlib
import sys

import bpy
from mathutils import Vector

ROOT = pathlib.Path(__file__).resolve().parents[3]
KIT = ROOT / "assets/kit"
argv = sys.argv[sys.argv.index("--") + 1:]
out, view, fence_glb, fence_masks, manifest_path = argv
man = json.loads(pathlib.Path(manifest_path).read_text())
pieces = {p["id"]: p for p in man["pieces"]}
sets = man["materials"]
FENCE_SLOTS = {"fence_paddock_timber": {"tileable": "plaster", "color": [0.25, 0.245, 0.255], "masks": True}}

# data/tiles/farm.ron heroes: (piece, (x, z), yaw_deg).
HEROES = [
    ("barn_red", (4.75, -8.227), 30.0),
    ("hay_bale_stack", (8.9, -2.4), 30.0),
    ("hay_bale_round", (7.5, -0.4), 15.0),
    ("hay_bale_round", (10.4, -1.4), 30.0),
    ("animal_cow", (0.25, 8.25), 255.0),
    ("animal_sheep", (-0.5, 10.75), 45.0),
    ("animal_sheep", (-2.25, 11.25), 345.0),
]

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
_images = {}


def image(path, non_color):
    key = (str(path), non_color)
    if key not in _images:
        img = bpy.data.images.load(str(path), check_existing=False)
        img.colorspace_settings.name = "Non-Color" if non_color else "sRGB"
        _images[key] = img
    return _images[key]


def node_mul(nt, a, b):
    mix = nt.nodes.new("ShaderNodeMix")
    mix.data_type = "RGBA"
    mix.blend_type = "MULTIPLY"
    ins = {x.identifier: x for x in mix.inputs}
    ins["Factor_Float"].default_value = 1.0
    for key, v in (("A_Color", a), ("B_Color", b)):
        if isinstance(v, tuple):
            ins[key].default_value = v
        else:
            nt.links.new(v, ins[key])
    return next(x for x in mix.outputs if x.identifier == "Result_Color")


def bind(mat, slot, maps):
    mat.use_backface_culling = "alphaCutoff" not in slot
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
        col = node_mul(nt, alb.outputs["Color"], (*slot.get("color", (1, 1, 1)), 1.0))
        orm = nt.nodes.new("ShaderNodeTexImage")
        orm.image = image(KIT / f"{st['maps']['orm']}.png", True)
        orm.projection = "BOX"
        orm.projection_blend = 0.15
        nt.links.new(mp.outputs["Vector"], orm.inputs["Vector"])
        sep = nt.nodes.new("ShaderNodeSeparateColor")
        nt.links.new(orm.outputs["Color"], sep.inputs["Color"])
        nt.links.new(sep.outputs["Green"], bsdf.inputs["Roughness"])
    elif slot.get("unique") and "albedo" in maps:
        uvn = nt.nodes.new("ShaderNodeUVMap")
        uvn.uv_map = "UVMap"
        alb = nt.nodes.new("ShaderNodeTexImage")
        alb.image = image(KIT / f"{maps['albedo']}.png", False)
        nt.links.new(uvn.outputs["UV"], alb.inputs["Vector"])
        col = alb.outputs["Color"]
        if "alphaCutoff" in slot:
            nt.links.new(alb.outputs["Alpha"], bsdf.inputs["Alpha"])
        bsdf.inputs["Roughness"].default_value = 0.85
    else:
        bsdf.inputs["Roughness"].default_value = slot.get("roughness", 0.8)
    masks = maps.get("masks_png")
    if slot.get("masks") and masks:
        uvn = nt.nodes.new("ShaderNodeUVMap")
        uvn.uv_map = "UVMap"
        mk = nt.nodes.new("ShaderNodeTexImage")
        mk.image = image(masks, True)
        nt.links.new(uvn.outputs["UV"], mk.inputs["Vector"])
        a = col if col is not None else (*slot["color"], 1.0)
        col = node_mul(nt, a, mk.outputs["Alpha"]) if not isinstance(a, tuple) else node_mul(nt, mk.outputs["Alpha"], a)
    if col is None:
        bsdf.inputs["Base Color"].default_value = (*slot["color"], 1.0)
    else:
        nt.links.new(col, bsdf.inputs["Base Color"])


def place(pid, glb, slots, maps, at, yaw_deg):
    before = set(bpy.data.objects)
    bpy.ops.import_scene.gltf(filepath=str(glb))
    new = [o for o in bpy.data.objects if o not in before]
    for o in new:
        if o.type == "MESH" and not o.name.startswith(f"{pid}_lod0"):
            bpy.data.objects.remove(o, do_unlink=True)
    bpy.context.view_layer.update()
    root = next(o for o in bpy.data.objects if o not in before and o.parent is None)
    root.location = Vector((at[0], -at[1], 0.0))
    root.rotation_mode = "XYZ"
    root.rotation_euler = (0.0, 0.0, math.radians(-yaw_deg))
    bpy.context.view_layer.update()
    lod0 = next(o for o in bpy.data.objects if o not in before and o.type == "MESH" and o.name.startswith(f"{pid}_lod0"))
    for s in lod0.material_slots:
        name = s.material.name.split(".")[0]
        if name in slots:
            s.material = s.material.copy()
            bind(s.material, slots[name], maps)
    return lod0


def kit_maps(pid):
    p = pieces[pid]
    maps = dict(p.get("maps", {}))
    if "masks" in maps:
        maps["masks_png"] = KIT / f"{maps['masks']}.png"
    return maps


place("meadow_base", KIT / pieces["meadow_base"]["glb"], pieces["meadow_base"]["slots"], kit_maps("meadow_base"),
      (0.0, 0.0), 0.0)
for pid, at, yaw in HEROES:
    place(pid, KIT / pieces[pid]["glb"], pieces[pid]["slots"], kit_maps(pid), at, yaw)
place("fence_paddock", pathlib.Path(fence_glb), FENCE_SLOTS, {"masks_png": fence_masks}, (0.0, 0.0), 0.0)

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
views = {
    # Blender (x, y, z) = runtime (x, -z, y).
    "hero": (Vector((17.0, -30.0, 13.0)), Vector((0.0, -6.0, 0.5)), 40),
    "close": (Vector((9.6, -5.2, 1.4)), Vector((12.2, -3.4, 0.6)), 32),
    "top": (Vector((0.0, -1.0, 46.0)), Vector((0.0, -0.999, 0.0)), 40),
}
loc, target, lens = views[view]
cam.data.lens = lens
cam.location = loc
cam.rotation_euler = (target - loc).to_track_quat("-Z", "Y").to_euler()
scn.render.filepath = out
bpy.ops.render.render(write_still=True)
print("still written", out)
