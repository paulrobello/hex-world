"""Textured headless EEVEE stills of train_carriage_b (evidence only: Blender's AgX view transform is
not Bevy's tonemapper; approval happens in engine). Pieces come from their committed GLBs, or for
train_* pieces from target/kit-stage with --stage, and draw as docs/checkpoints/train/still.py does
(car_material.wesl's paint formula, masks A on UV0; rail and meadow slots box-projected at tileM).

Views: `tail` (three-quarter from the veranda end, sun side), `side` (the sun side, level with the
windows), `pair` (train_carriage and train_carriage_b side by side on two tracks), `straight` (the
consist engine, train_carriage, train_carriage_b on the straight, buffers touching), `bend` (the
consist on the 8 m bend, poses from ../train/consist.py with the brake third last), `close` (the
veranda about 4 m out).
Usage: Blender -b --factory-startup --python-exit-code 1 -P docs/checkpoints/train_carriage_b/still.py --
       <out.png> <view> [--stage]"""
import json
import math
import pathlib
import sys

import bpy
from mathutils import Vector

ROOT = pathlib.Path(__file__).resolve().parents[3]
sys.path.insert(0, str(ROOT / "docs/checkpoints/train"))
import consist  # noqa: E402

KIT = ROOT / "assets/kit"
args = sys.argv[sys.argv.index("--") + 1:]
out, view = args[0], args[1]
stage = "--stage" in args
man = json.loads((KIT / "manifest.json").read_text())
pieces = {p["id"]: p for p in man["pieces"]}
sets = man["materials"]
# sRGB liveries from hex_play::trains::LIVERIES.
GREEN, CRIMSON, BLUE, CHOCOLATE = (0x2f, 0x5d, 0x3a), (0x7a, 0x1f, 0x22), (0x2c, 0x4f, 0x8a), (0x5a, 0x34, 0x22)
SLOTS = (("body", 0.45), ("glass", 0.05), ("tyre", 0.6))


def lin(c):
    c = c / 255.0
    return c / 12.92 if c <= 0.04045 else ((c + 0.055) / 1.055) ** 2.4


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


def mul(nt, a, b):
    m = nt.nodes.new("ShaderNodeMix")
    m.data_type = "RGBA"
    m.blend_type = "MULTIPLY"
    ins = {x.identifier: x for x in m.inputs}
    ins["Factor_Float"].default_value = 1.0
    for key, v in (("A_Color", a), ("B_Color", b)):
        if isinstance(v, tuple):
            ins[key].default_value = (*v, 1.0)
        else:
            nt.links.new(v, ins[key])
    return next(x for x in m.outputs if x.identifier == "Result_Color")


def bind(mat, slot, masks_png, paint):
    mat.use_backface_culling = True
    nt = mat.node_tree
    bsdf = next(n for n in nt.nodes if n.type == "BSDF_PRINCIPLED")
    base = slot.get("color") or (1.0, 1.0, 1.0)
    col = None
    if paint is not None:  # car_material.wesl: base * COLOR_0.rgb, then mix(.., .. * paint, COLOR_0.a)
        attr = nt.nodes.new("ShaderNodeVertexColor")
        attr.layer_name = "Color"
        rgb = mul(nt, attr.outputs["Color"], tuple(base))
        mix = nt.nodes.new("ShaderNodeMix")
        mix.data_type = "RGBA"
        ins = {x.identifier: x for x in mix.inputs}
        nt.links.new(attr.outputs["Alpha"], ins["Factor_Float"])
        nt.links.new(rgb, ins["A_Color"])
        nt.links.new(mul(nt, rgb, tuple(paint)), ins["B_Color"])
        col = next(x for x in mix.outputs if x.identifier == "Result_Color")
        bsdf.inputs["Roughness"].default_value = slot.get("roughness", 0.5)
    elif "tileable" in slot:
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
        col = mul(nt, alb.outputs["Color"], tuple(slot.get("color", (1, 1, 1))))
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
    if masks_png and slot.get("masks") and pathlib.Path(masks_png).exists():
        uvn = nt.nodes.new("ShaderNodeUVMap")
        uvn.uv_map = "UVMap"
        mk = nt.nodes.new("ShaderNodeTexImage")
        mk.image = image(masks_png, True)
        nt.links.new(uvn.outputs["UV"], mk.inputs["Vector"])
        col = mul(nt, col if col is not None else tuple(base), mk.outputs["Alpha"])
    if col is None:
        bsdf.inputs["Base Color"].default_value = (*base, 1.0)
    else:
        nt.links.new(col, bsdf.inputs["Base Color"])


def place(pid, at=(0.0, 0.0, 0.0), yaw_deg=0.0, paint=None):
    before = set(bpy.data.objects)
    staged = stage and pid.startswith("train_") and (ROOT / f"target/kit-stage/{pid}/{pid}.glb").exists()
    if staged:
        glb = ROOT / f"target/kit-stage/{pid}/{pid}.glb"
        masks = ROOT / f"target/kit-stage/{pid}/{pid}.masks.png"
    else:
        glb = KIT / pieces[pid]["glb"]
        masks = KIT / f"{pieces[pid]['maps']['masks']}.png" if "maps" in pieces[pid] else None
    bpy.ops.import_scene.gltf(filepath=str(glb))
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
    lod0 = next(o for o in bpy.data.objects if o not in before and o.name.startswith(f"{pid}_lod0"))
    slots = pieces[pid]["slots"] if pid in pieces else {f"{pid}_{s}": {"roughness": r, "masks": True}
                                                           for s, r in SLOTS}
    vc = pid.startswith(("train_", "car_"))
    p = tuple(lin(c) for c in paint) if paint else (1.0, 1.0, 1.0)
    for s in lod0.material_slots:
        name = s.material.name.split(".")[0]
        if name in slots:
            bind(s.material, slots[name], masks, p if vc else None)
    return lod0


R = consist.RAIL_TOP
E2C = consist.ENGINE_TO_CARRIAGE
C2C = consist.CARRIAGE_TO_CARRIAGE
if view in ("tail", "side", "close"):
    place("meadow_base")
    place("rail_straight_center")
    place("train_carriage_b", (0.0, 0.0, R), 0.0, CRIMSON)
    place("train_carriage", (C2C, 0.0, R), 0.0, CRIMSON)
elif view == "pair":
    place("meadow_base")
    place("rail_straight_center")
    place("rail_straight_center", (0.0, 6.0, 0.0))
    place("train_carriage", (0.0, 6.0, R), 0.0, BLUE)
    place("train_carriage_b", (0.0, 0.0, R), 0.0, BLUE)
elif view == "straight":
    place("meadow_base")
    place("rail_straight_center")
    x = 5.5
    place("train_engine", (x, 0.0, R), 0.0, GREEN)
    place("train_carriage", (x - E2C, 0.0, R), 0.0, GREEN)
    place("train_carriage_b", (x - E2C - C2C, 0.0, R), 0.0, GREEN)
elif view == "bend":
    place("meadow_base")
    place("rail_bend_60_center")
    poses = consist.bend_poses_blender()
    for (pid, x, y, yaw), last in zip(poses, (False, False, True)):
        place("train_carriage_b" if last else pid, (x, y, R), yaw, CHOCOLATE)

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
    "tail": (Vector((-9.5, -8.5, 4.6)), Vector((-0.6, 0.0, 2.0))),
    "side": (Vector((0.5, -13.0, 2.6)), Vector((0.5, 0.0, 2.0))),
    "pair": (Vector((-11.0, -10.0, 8.5)), Vector((0.0, 3.0, 1.6))),
    "straight": (Vector((-14.0, -22.0, 10.0)), Vector((-3.0, 0.0, 1.5))),
    "bend": (Vector((-6.0, -26.0, 22.0)), Vector((7.0, -4.5, 0.5))),
    "close": (Vector((-6.6, -3.2, 2.3)), Vector((-3.0, 0.0, 2.2))),
}
loc, target = views[view]
cam.location = loc
cam.rotation_euler = (target - loc).to_track_quat("-Z", "Y").to_euler()
scn.render.filepath = out
bpy.ops.render.render(write_still=True)
print("still written", out)
