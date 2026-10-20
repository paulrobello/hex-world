"""Textured headless EEVEE stills of the train (evidence only: Blender's AgX view transform is not
Bevy's tonemapper; approval happens in engine). Pieces are imported from their committed GLBs (or a
staged one under target/kit-stage with --stage) and drawn the way the runtime does:
- train and car slots draw like `car_material.wesl`: COLOR_0.rgb times the slot colour, and where
  COLOR_0.a = 1 (the paint mask) times the instance paint colour; diffuse times the masks' A on UV0;
- rail and meadow slots as docs/checkpoints/rail/still.py (tileable sets box-projected at tileM).

Views: `engine` and `carriage` (three-quarter, sun side), `straight` (the consist on the straight),
`bend` (the consist on the 8 m bend, poses from consist.py), `close` (4 m out at the engine's front),
`scale` (the engine beside a sedan and a cottage), `paint-a` / `paint-b` (two runtime body colours).
Usage: Blender -b --factory-startup --python-exit-code 1 -P docs/checkpoints/train/still.py --
       <out.png> <view> [--stage]"""
import json
import math
import pathlib
import sys

import bpy
from mathutils import Vector

ROOT = pathlib.Path(__file__).resolve().parents[3]
sys.path.insert(0, str(pathlib.Path(__file__).parent))
import consist  # noqa: E402

KIT = ROOT / "assets/kit"
args = sys.argv[sys.argv.index("--") + 1:]
out, view = args[0], args[1]
stage = "--stage" in args
man = json.loads((KIT / "manifest.json").read_text())
pieces = {p["id"]: p for p in man["pieces"]}
sets = man["materials"]
# sRGB paint colours from hex_play::cars::PALETTE-like choices (the train takes its own at runtime).
PAINTS = {"engine": (0x3a, 0x5a, 0x40), "carriage": (0x6e, 0x2a, 0x22), "paint-a": (0x24, 0x3f, 0x6b),
          "paint-b": (0xb8, 0x8a, 0x2e)}
FALLBACK = {"car_sedan_body": (1.0, 1.0, 1.0), "car_sedan_glass": (1.0, 1.0, 1.0), "car_sedan_tyre": (1.0, 1.0, 1.0)}


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


def bind(mat, name, slot, masks_png, paint):
    mat.use_backface_culling = True
    nt = mat.node_tree
    bsdf = next(n for n in nt.nodes if n.type == "BSDF_PRINCIPLED")
    base = slot.get("color") or FALLBACK.get(name, (1.0, 1.0, 1.0))
    col = None
    vc = paint is not None
    if vc:  # car_material.wesl: base * COLOR_0.rgb, then mix(base, base * paint, COLOR_0.a)
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
    glb = KIT / pieces[pid]["glb"] if pid in pieces else None
    masks = KIT / f"{pieces[pid]['maps']['masks']}.png" if pid in pieces and "maps" in pieces[pid] else None
    if stage and pid.startswith("train_"):
        glb = ROOT / f"target/kit-stage/{pid}/{pid}.glb"
        masks = ROOT / f"target/kit-stage/{pid}/{pid}.masks.png"
    bpy.ops.import_scene.gltf(filepath=str(glb))
    new = [o for o in bpy.data.objects if o not in before]
    for o in new:
        if o.type == "MESH" and o.name.split(".")[0] != f"{pid}_lod0":  # a second import is <name>.001
            bpy.data.objects.remove(o, do_unlink=True)
    bpy.context.view_layer.update()
    root = next(o for o in bpy.data.objects if o not in before and o.parent is None)
    root.location = Vector(at)
    root.rotation_mode = "XYZ"
    root.rotation_euler = (0.0, 0.0, math.radians(yaw_deg))
    bpy.context.view_layer.update()
    lod0 = next(o for o in bpy.data.objects if o not in before and o.name.startswith(f"{pid}_lod0"))
    slots = pieces[pid]["slots"] if pid in pieces else {f"{pid}_{s}": {"roughness": r, "masks": True}
                                                           for s, r in (("body", 0.45), ("glass", 0.05), ("tyre", 0.6))}
    vc = pid.startswith(("train_", "car_"))
    p = tuple(lin(c) for c in paint) if paint else (1.0, 1.0, 1.0)
    for s in lod0.material_slots:
        name = s.material.name.split(".")[0]
        if name in slots:
            bind(s.material, name, slots[name], masks, p if vc else None)
    return lod0


def consist_on(track, poses, paints):
    place("meadow_base")
    place(track)
    for (pid, x, y, yaw), paint in zip(poses, paints):
        place(pid, (x, y, consist.RAIL_TOP), yaw, paint)


eng, car = PAINTS["engine"], PAINTS["carriage"]
if view in ("engine", "carriage", "paint-a", "paint-b"):
    place("meadow_base")
    place("rail_straight_center")
    pid = "train_carriage" if view == "carriage" else "train_engine"
    paint = PAINTS[view] if view.startswith("paint") else (eng if view == "engine" else car)
    place(pid, (0.0, 0.0, consist.RAIL_TOP), 0.0, paint)
    if view.startswith("paint"):
        place("train_carriage", (-consist.ENGINE_TO_CARRIAGE, 0.0, consist.RAIL_TOP), 0.0, PAINTS[view])
elif view in ("straight", "close"):
    consist_on("rail_straight_center", consist.straight_poses(), (eng, car, car))
elif view == "bend":
    consist_on("rail_bend_60_center", consist.bend_poses_blender(), (eng, car, car))
elif view == "scale":
    place("meadow_base")
    place("rail_straight_center")
    place("train_engine", (2.0, 0.0, consist.RAIL_TOP), 0.0, eng)
    place("car_sedan", (2.0, 5.2, -0.12), 0.0, (0x8c, 0x1c, 0x1c))
    place("house_cottage", (-7.0, 10.5, -0.15), yaw_deg=90.0)

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
    "engine": (Vector((9.0, -10.5, 4.5)), Vector((0.3, 0.0, 1.9))),
    "carriage": (Vector((8.0, -9.5, 4.0)), Vector((0.0, 0.0, 1.8))),
    "paint-a": (Vector((6.0, -17.0, 5.0)), Vector((-3.6, 0.0, 1.8))),
    "paint-b": (Vector((6.0, -17.0, 5.0)), Vector((-3.6, 0.0, 1.8))),
    "straight": (Vector((14.0, -22.0, 10.0)), Vector((-2.0, 0.0, 1.5))),
    "bend": (Vector((-6.0, -26.0, 22.0)), Vector((7.0, -4.5, 0.5))),
    "close": (Vector((consist.straight_poses()[0][1] + 6.0, -3.6, 1.6)),
              Vector((consist.straight_poses()[0][1] + 2.5, 0.0, 1.4))),
    "scale": (Vector((15.0, -12.0, 5.0)), Vector((-1.0, 4.0, 1.6))),
}
loc, target = views[view]
cam.location = loc
cam.rotation_euler = (target - loc).to_track_quat("-Z", "Y").to_euler()
scn.render.filepath = out
bpy.ops.render.render(write_still=True)
print("still written", out)
