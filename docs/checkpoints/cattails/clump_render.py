"""Close-up Blender stills of one reed clump of a river dressing GLB, lit from the camera side and
from behind (the back-lit still checks that blade back sheets shade as lit, not black). Slot colours
come from assets/kit/manifest.json (linear), so the stills match the engine's flat slot colours;
Cycles CPU, so a still never holds the GPU while an engine shot runs.
Usage: Blender -b --factory-startup --python-exit-code 1 -P clump_render.py -- <glb> <piece> <out-prefix>"""
import json
import math
import pathlib
import sys

import bpy
from mathutils import Vector

glb, piece, prefix = sys.argv[sys.argv.index("--") + 1:][:3]
root = pathlib.Path(__file__).resolve().parents[3]
slots = next(p for p in json.loads((root / "assets/kit/manifest.json").read_text())["pieces"]
             if p["id"] == piece)["slots"]

bpy.ops.wm.read_factory_settings(use_empty=True)
bpy.ops.import_scene.gltf(filepath=glb)
for o in list(bpy.data.objects):
    if o.type == "MESH" and not o.name.endswith("_lod0"):
        bpy.data.objects.remove(o)
for m in bpy.data.materials:
    s = slots.get(m.name)
    if s and m.node_tree:
        bsdf = next(n for n in m.node_tree.nodes if n.type == "BSDF_PRINCIPLED")
        bsdf.inputs["Base Color"].default_value = (*s["color"], 1.0)
        bsdf.inputs["Roughness"].default_value = s.get("roughness", 0.8)

# The clump at Blender (-0.66, 5.34) on river_straight_center's +y bank (the in-engine close-up's).
clump = Vector((-0.66, 5.34, -0.5))
ground = bpy.data.meshes.new("ground")
ground.from_pydata([(clump.x - 4, clump.y - 4, clump.z), (clump.x + 4, clump.y - 4, clump.z),
                    (clump.x + 4, clump.y + 4, clump.z), (clump.x - 4, clump.y + 4, clump.z)], [], [(0, 1, 2, 3)])
g = bpy.data.objects.new("ground", ground)
bpy.context.scene.collection.objects.link(g)
gm = bpy.data.materials.new("ground")
gm.use_nodes = True
gm.node_tree.nodes["Principled BSDF"].inputs["Base Color"].default_value = (0.09, 0.12, 0.05, 1.0)
ground.materials.append(gm)

s = bpy.context.scene
s.render.engine = "CYCLES"
s.cycles.device = "CPU"
s.cycles.samples = 48
s.render.resolution_x, s.render.resolution_y = 1100, 1100
s.render.use_stamp_date = s.render.use_stamp_render_time = False
s.view_settings.view_transform = "AgX"
s.world = bpy.data.worlds.new("w")
s.world.color = (0.35, 0.42, 0.55)
cam = bpy.data.objects.new("cam", bpy.data.cameras.new("cam"))
cam.data.lens = 40
s.collection.objects.link(cam)
s.camera = cam
sun = bpy.data.objects.new("sun", bpy.data.lights.new("sun", "SUN"))
sun.data.energy = 3.5
s.collection.objects.link(sun)
look = clump + Vector((0, 0, 0.85))
cam.location = clump + Vector((0.4, 2.6, 1.3))
cam.rotation_euler = (look - cam.location).to_track_quat("-Z", "Y").to_euler()
for name, az in (("front-lit", 200.0), ("back-lit", 20.0)):
    sun.rotation_euler = (math.radians(50), 0, math.radians(az))
    s.render.filepath = f"{prefix}-{name}.png"
    bpy.ops.render.render(write_still=True)
    print("wrote", s.render.filepath)
