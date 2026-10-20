"""Headless EEVEE review stills for the boats and the landing stage (SKILL.md, "Owner checkpoints":
a textured still binds the staged maps, which the GLB does not embed).

Run: "$BLENDER" -b --factory-startup --python-exit-code 1 -P docs/checkpoints/boats/still.py -- <shots.json>

shots.json: {"out": dir, "size": [w, h], "scenes": [{"name", "pieces": [{"id", "glb", "masks", "at": [x, y, z],
"yaw": deg}], "water": bool, "bank": bool, "views": [{"name", "eye": [x, y, z], "look": [x, y, z], "lens": mm}]}]}

Every slot colour comes from the GLB's own material (the generator's slot colour); masks.A (baked AO)
multiplies it, as the runtime's masked path does. `water` adds a translucent plane at the origin
plane z = 0 (the water at rest) over a sand floor 1.6 m down; `bank` adds the §2 river profile
(docs/kit/water-contract.md) as a grass and earth strip, with the bank's waterline on local x = 0
and the channel centre line at x = +4 (the landing stage's frame)."""
import json
import os
import math
import sys

import bpy
from mathutils import Vector

LAND = 1.4


def smooth(e0, e1, x):
    t = min(max((x - e0) / (e1 - e0), 0.0), 1.0)
    return t * t * (3 - 2 * t)


def river_y(d):
    """Water contract §2 bed height relative to the water surface (runtime y + 1.4)."""
    w = 4.33
    bed = -1.5 * max(0.0, 1 - (d / w) ** 2) - 0.05
    bank = smooth(0.7 * w, w + 2.5, d)
    return min(LAND, bed + (LAND - bed) * bank)


def mat(name, rgb, rough=0.6, alpha=1.0):
    m = bpy.data.materials.new(name)
    nt = m.node_tree
    b = nt.nodes["Principled BSDF"]
    b.inputs["Base Color"].default_value = (*rgb, 1.0)
    b.inputs["Roughness"].default_value = rough
    if alpha < 1.0:
        b.inputs["Alpha"].default_value = alpha
        m.surface_render_method = "BLENDED"
    return m


def bank_strip():
    """The river cross-section from d = -2 (far side of the centre line) to d = 12, along local x
    (x = 4 - d), 12 m along y, faces by height: earth below 1.0 m over the water, grass above."""
    ds = [i * 0.25 for i in range(-8, 49)]
    ys = [-6.0 + i * 1.0 for i in range(13)]
    verts, faces = [], []
    for y in ys:
        for d in ds:
            verts.append((4.0 - d, y, river_y(abs(d))))
    n = len(ds)
    for j in range(len(ys) - 1):
        for i in range(n - 1):
            a = j * n + i
            faces.append((a, a + n, a + n + 1, a + 1))
    me = bpy.data.meshes.new("bank")
    me.from_pydata(verts, [], faces)
    me.materials.append(mat("earth", (0.16, 0.10, 0.055), 0.9))
    me.materials.append(mat("grass", (0.10, 0.20, 0.04), 0.9))
    for p in me.polygons:
        z = sum(me.vertices[v].co.z for v in p.vertices) / 4
        p.material_index = 1 if z > 1.05 else 0
        p.use_smooth = True
    o = bpy.data.objects.new("bank", me)
    bpy.context.scene.collection.objects.link(o)
    return o


def water(size=60.0, floor=-1.6, bank=False):
    bpy.ops.mesh.primitive_plane_add(size=size, location=(0, 0, 0))
    w = bpy.context.active_object
    w.data.materials.append(mat("water", (0.02, 0.12, 0.13), 0.05, 0.72))
    if not bank:
        bpy.ops.mesh.primitive_plane_add(size=size, location=(0, 0, floor))
        bpy.context.active_object.data.materials.append(mat("sand", (0.35, 0.30, 0.2), 0.9))


def import_piece(p):
    before = set(bpy.data.objects)
    bpy.ops.import_scene.gltf(filepath=os.path.abspath(p["glb"]))
    new = [o for o in bpy.data.objects if o not in before]
    roots = [o for o in new if o.parent is None]
    img = bpy.data.images.load(os.path.abspath(p["masks"])) if p.get("masks") else None
    if img:
        img.colorspace_settings.name = "Non-Color"
    for o in new:
        if o.name.endswith("_collider") or (o.type == "MESH" and not o.name.split(".")[0].endswith("lod0")):
            o.hide_render = True
        if o.type != "MESH" or img is None:
            continue
        for slot in o.material_slots:
            m = slot.material
            if m is None or m.get("_masked"):
                continue
            m["_masked"] = True
            nt = m.node_tree
            b = next(n for n in nt.nodes if n.type == "BSDF_PRINCIPLED")
            col = tuple(b.inputs["Base Color"].default_value)
            tex = nt.nodes.new("ShaderNodeTexImage")
            tex.image = img
            mul = nt.nodes.new("ShaderNodeMixRGB")
            mul.blend_type = "MULTIPLY"
            mul.inputs["Fac"].default_value = 1.0
            mul.inputs["Color1"].default_value = col
            nt.links.new(tex.outputs["Alpha"], mul.inputs["Color2"])
            nt.links.new(mul.outputs["Color"], b.inputs["Base Color"])
    # Part poses about each part node's local up axis: "oar" (port +, starboard -, so a positive yaw
    # trails both blades aft), "main" and "jib" (positive swings the sail to port).
    pose = p.get("pose", {})
    for o in new:
        name = o.name.split(".")[0]
        for key, sign in (("_oar_port_", 1), ("_oar_stbd_", -1), ("_main_", 1), ("_jib_", 1)):
            k = key.strip("_").split("_")[0]
            if key in name and k in pose:
                q = o.rotation_quaternion.copy() if o.rotation_mode == "QUATERNION" else o.rotation_euler.to_quaternion()
                o.rotation_mode = "QUATERNION"
                from mathutils import Quaternion
                o.rotation_quaternion = q @ Quaternion((0, 0, 1), math.radians(sign * pose[k]))
    for r in roots:
        r.rotation_mode = "XYZ"
        r.location = p.get("at", (0, 0, 0))
        r.rotation_euler = (0, 0, math.radians(p.get("yaw", 0.0)))
    return new


def setup(size):
    scn = bpy.context.scene
    scn.render.engine = "BLENDER_EEVEE"
    scn.render.resolution_x, scn.render.resolution_y = size
    scn.render.use_stamp_date = False
    scn.render.use_stamp_render_time = False
    scn.view_settings.view_transform = "AgX"
    world = bpy.data.worlds.new("sky")
    scn.world = world
    bg = world.node_tree.nodes["Background"]
    bg.inputs["Color"].default_value = (0.55, 0.62, 0.72, 1.0)
    bg.inputs["Strength"].default_value = 0.9
    sun = bpy.data.lights.new("sun", "SUN")
    sun.energy = 3.2
    so = bpy.data.objects.new("sun", sun)
    so.rotation_euler = (math.radians(40), 0.0, math.radians(35))
    scn.collection.objects.link(so)
    cam = bpy.data.objects.new("cam", bpy.data.cameras.new("cam"))
    scn.collection.objects.link(cam)
    scn.camera = cam
    return cam


def main():
    cfg = json.load(open(sys.argv[sys.argv.index("--") + 1]))
    cfg["out"] = os.path.abspath(cfg["out"])
    for s in cfg["scenes"]:
        bpy.ops.wm.read_factory_settings(use_empty=True)
        cam = setup(cfg.get("size", [1200, 800]))
        for p in s["pieces"]:
            import_piece(p)
        if s.get("bank"):
            bank_strip()
        if s.get("water"):
            water(bank=s.get("bank", False))
        if not s.get("water") and not s.get("bank"):
            bpy.ops.mesh.primitive_plane_add(size=80, location=(0, 0, s.get("floor", -0.25)))
            bpy.context.active_object.data.materials.append(mat("floor", (0.32, 0.32, 0.32), 0.8))
        for v in s["views"]:
            cam.location = v["eye"]
            d = Vector(v["look"]) - Vector(v["eye"])
            cam.rotation_mode = "QUATERNION"
            cam.rotation_quaternion = d.to_track_quat("-Z", "Y")
            cam.data.lens = v.get("lens", 50)
            bpy.context.scene.render.filepath = f"{cfg['out']}/{s['name']}-{v['name']}.png"
            bpy.ops.render.render(write_still=True)


main()
