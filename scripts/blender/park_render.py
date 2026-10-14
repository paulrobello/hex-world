"""Review stills of a pad's parking spots (PRD §7.1): the staged GLB with a 5.4 x 2.0 m outline and a
nose arrow at every anchor-park-<n>, and the house footprints as low grey blocks for context.

    "$BLENDER" -b --factory-startup --python-exit-code 1 -P scripts/blender/park_render.py -- house_pad

Evidence only, outside the generator and the GLB. Committed JPEGs go to
assets-source/kit/<id>/renders/parking_{top,three_quarter}.jpg, lossless PNGs to target/kit-renders/<id>/."""
import math
import pathlib
import sys

import bmesh
import bpy
from mathutils import Vector

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
from lib import gate, scene  # noqa: E402

piece = sys.argv[sys.argv.index("--") + 1]
stage = pathlib.Path("target/kit-stage") / piece
objs = gate.import_glb(stage / f"{piece}.glb")
# Houses of data/tiles/house_cluster.ron in Blender (x, y): centre, footprint along x and y.
HOUSES = (((-0.5, 7.0), 6.4, 4.4), ((-0.5, -7.0), 6.4, 4.4), ((-7.5, 0.0), 5.0, 7.2))


def emissive(name, rgb, strength=2.0):
    m = bpy.data.materials.new(name)
    m.use_nodes = True
    bsdf = m.node_tree.nodes["Principled BSDF"]
    bsdf.inputs["Base Color"].default_value = (*rgb, 1.0)
    bsdf.inputs["Emission Color"].default_value = (*rgb, 1.0)
    bsdf.inputs["Emission Strength"].default_value = strength
    return m


def mesh_obj(name, bm, mat):
    me = bpy.data.meshes.new(name)
    bm.to_mesh(me)
    bm.free()
    me.materials.append(mat)
    return scene.link(bpy.data.objects.new(name, me))


spot_mat = emissive("park_spot", (1.0, 0.8, 0.05))
house_mat = emissive("park_house", (0.45, 0.45, 0.45), 0.0)
marks = []
for o in objs:
    if not o.name.startswith("anchor-park-"):
        continue
    length, width = float(o["length"]), float(o["width"])
    mw = o.matrix_world
    fwd = (mw.to_3x3() @ Vector((1, 0, 0))).normalized()
    side = Vector((-fwd.y, fwd.x, 0.0))
    c = mw.translation + Vector((0, 0, 0.03))
    bm = bmesh.new()
    t = 0.12
    for a in (1, -1):  # long sides, then short sides, as thin quads
        for p0, p1, n in ((c + side * a * width / 2 - fwd * length / 2, c + side * a * width / 2 + fwd * length / 2, side),
                          (c + fwd * a * length / 2 - side * width / 2, c + fwd * a * length / 2 + side * width / 2, fwd)):
            q = [p0 - n * t / 2, p1 - n * t / 2, p1 + n * t / 2, p0 + n * t / 2]
            bm.faces.new([bm.verts.new(v) for v in q])
    tip = c + fwd * (length / 2 - 0.3)
    bm.faces.new([bm.verts.new(v) for v in (tip, tip - fwd * 1.2 + side * 0.6, tip - fwd * 1.2 - side * 0.6)])
    marks.append(mesh_obj(f"{o.name}_mark_lod0", bm, spot_mat))  # render_six_views draws *_lod0 only
    print(f"{o.name}: runtime ({c.x:.3f}, {-c.y:.3f}) heading {math.degrees(math.atan2(-fwd.y, fwd.x)):.1f} deg")
for i, ((x, y), sx, sy) in enumerate(HOUSES):
    bm = bmesh.new()
    bmesh.ops.create_cube(bm, size=1.0)
    for v in bm.verts:
        v.co = Vector((x + v.co.x * sx, y + v.co.y * sy, 0.15 + v.co.z * 0.3))
    marks.append(mesh_obj(f"house_{i}_lod0", bm, house_mat))
gate.VIEWS = {"parking_top": (0, 0, 1), "parking_three_quarter": (0.8, -0.8, 0.6)}
print("\n".join(gate.render_six_views(objs + marks, pathlib.Path("assets-source/kit") / piece / "renders",
                                      png_dir=pathlib.Path("target/kit-renders") / piece)))
