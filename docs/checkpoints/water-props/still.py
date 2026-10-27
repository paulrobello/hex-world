"""Headless EEVEE stills of the B3 water props (evidence only: Blender's view transform is not Bevy's).
LOD0 of lily_cluster, lily_cluster_b, lily_cluster_c and reed_bed from a GLB folder (the staged
target/kit-stage/<id>/ or assets/kit/p1/<id>/), colour-only slots as the GLB carries them, on a dark
water plane at the pads' pivot level with the river bank's profile under the reed bed, beside a 1.7 m
figure. Usage (from the repo root):
  Blender -b --factory-startup --python-exit-code 1 -P docs/checkpoints/water-props/still.py -- <glb root> <out dir>
<glb root> holds <id>/<id>.glb."""
import math
import pathlib
import sys

import bpy
from mathutils import Vector

args = sys.argv[sys.argv.index("--") + 1:]
glb_root, out_dir = pathlib.Path(args[0]), pathlib.Path(args[1])
out_dir.mkdir(parents=True, exist_ok=True)
PIECES = ("lily_cluster", "lily_cluster_b", "lily_cluster_c", "reed_bed")

bpy.ops.wm.read_factory_settings(use_empty=True)
scn = bpy.context.scene
scn.render.engine = "BLENDER_EEVEE"
scn.render.resolution_x, scn.render.resolution_y = 1400, 900
scn.render.use_stamp_date = False
scn.render.use_stamp_render_time = False
scn.view_settings.view_transform = "AgX"
world = bpy.data.worlds.new("still")
world.color = (0.55, 0.62, 0.72)
scn.world = world


def principled(mat):
    return next(n for n in mat.node_tree.nodes if n.type == "BSDF_PRINCIPLED")


def load(piece):
    before = set(bpy.data.objects)
    bpy.ops.import_scene.gltf(filepath=str(glb_root / piece / f"{piece}.glb"))
    new = [o for o in bpy.data.objects if o not in before]
    keep = next(o for o in new if o.type == "MESH" and o.name.startswith(f"{piece}_lod0"))
    for o in new:
        if o is not keep and o.type == "MESH":
            bpy.data.objects.remove(o, do_unlink=True)
    keep.matrix_world = keep.matrix_world.copy()
    keep.parent = None
    return keep


def plane(name, size, z, color, rough, alpha=1.0):
    bpy.ops.mesh.primitive_plane_add(size=size, location=(0, 0, z))
    o = bpy.context.object
    o.name = name
    m = bpy.data.materials.new(name)
    b = principled(m)
    b.inputs["Base Color"].default_value = color
    b.inputs["Roughness"].default_value = rough
    if alpha < 1.0:
        b.inputs["Alpha"].default_value = alpha
        m.surface_render_method = "BLENDED"
    o.data.materials.append(m)
    return o


def shoot(name, loc, target, lens):
    cam.location = Vector(loc)
    cam.data.lens = lens
    cam.rotation_euler = (Vector(target) - cam.location).to_track_quat("-Z", "Y").to_euler()
    scn.render.filepath = str(out_dir / f"{name}.png")
    bpy.ops.render.render(write_still=True)


# Water just under the pads (their pivot is 0.01 m above the water), a bed 0.6 m below it.
water = plane("water", 80.0, -0.01, (0.05, 0.11, 0.12, 1), 0.08, alpha=0.82)
bed = plane("bed", 80.0, -0.6, (0.16, 0.14, 0.1, 1), 0.95)
sun = bpy.data.objects.new("sun", bpy.data.lights.new("sun", "SUN"))
sun.data.energy = 4.0
sun.rotation_euler = (math.radians(45), 0, math.radians(35))
scn.collection.objects.link(sun)
cam = bpy.data.objects.new("cam", bpy.data.cameras.new("cam"))
scn.collection.objects.link(cam)
scn.camera = cam
bpy.ops.mesh.primitive_cylinder_add(radius=0.12, depth=1.7, location=(0, 0, 0.85))
fig = bpy.context.object
fm = bpy.data.materials.new("figure")
principled(fm).inputs["Base Color"].default_value = (0.25, 0.25, 0.3, 1)
fig.data.materials.append(fm)

objs = {p: load(p) for p in PIECES}


def show(only):
    for p, o in objs.items():
        o.hide_render = p not in only
    fig.hide_render = "fig" not in only


spots = {"lily_cluster": (-3.4, -0.6), "lily_cluster_b": (-0.4, -0.9), "lily_cluster_c": (2.6, -0.5),
         "reed_bed": (0.0, 2.6)}
for p, o in objs.items():
    o.location = (*spots[p], 0.0)
fig.location = (4.6, 1.4, 0.85)
show((*PIECES, "fig"))
shoot("lineup", (0.4, -9.6, 4.6), (0.2, 0.6, 0.5), 30)
for p, o in objs.items():
    o.location = (0, 0, 0)
    show((p,))
    if p == "reed_bed":
        shoot(f"{p}-eevee", (3.4, -5.6, 1.9), (0, 0, 1.1), 35)
        shoot(f"{p}-eevee-low", (0.0, -4.6, 0.4), (0, 0, 1.0), 30)
    else:
        shoot(f"{p}-eevee", (1.7, -2.4, 1.6), (0, 0, 0.0), 40)
print("stills written", out_dir, PIECES)
