# tree_hero_oak live authoring, step 1: run in the owner's live Blender through
# scripts/blender/mcp_live.py exec. Creates or rebuilds ONLY tree_hero_oak-prefixed data in a scene
# named tree_hero_oak. The owner's "Scene" and its data are never touched.
import importlib
import json
import math
import os
import sys

import bpy
from mathutils import Vector

# REPO (the checkout root) is prepended by scripts/blender/mcp_live.py exec.
sys.path.insert(0, f"{REPO}/scripts/blender/kit")  # noqa: F821
import _leaves  # noqa: E402
import _tree  # noqa: E402

importlib.reload(_leaves)
importlib.reload(_tree)
P = _tree.PIECE

scn = bpy.data.scenes.get(P) or bpy.data.scenes.new(P)
# Rebuild: remove our own objects only (prefix-owned, in our scene).
for o in list(scn.objects):
    if o.name.startswith(P):
        bpy.data.objects.remove(o, do_unlink=True)
for coll in (bpy.data.meshes, bpy.data.lights, bpy.data.cameras):
    for d in list(coll):
        if d.name.startswith(P) and d.users == 0:
            coll.remove(d)

win = bpy.context.window_manager.windows[0]
win.scene = scn  # the owner watches this scene in the viewport (owner request)
scn.unit_settings.system = "METRIC"
scn.unit_settings.scale_length = 1.0
scn.render.engine = "BLENDER_EEVEE"
scn.render.resolution_x, scn.render.resolution_y = 1600, 1200
scn.view_settings.view_transform = "AgX"

# World: the look-dev noon HDRI as ambient.
world = bpy.data.worlds.get(f"{P}_world") or bpy.data.worlds.new(f"{P}_world")
nt = world.node_tree
for n in list(nt.nodes):
    nt.nodes.remove(n)
env = nt.nodes.new("ShaderNodeTexEnvironment")
HDRI = f"{REPO}/assets-source/hdri/kloofendal_48d_partly_cloudy_puresky_2k.hdr"  # noqa: F821
if not os.path.isfile(HDRI):
    raise FileNotFoundError(f"{HDRI} missing: run make fetch-hdri (gitignored; URL and SHA-256 in lookdev.json)")
env.image = bpy.data.images.load(HDRI,
                                 check_existing=True)
env.image.name = f"{P}_hdri_noon"  # owner rule O1: every datablock carries the piece prefix
bg = nt.nodes.new("ShaderNodeBackground")
bg.inputs["Strength"].default_value = 0.6
out = nt.nodes.new("ShaderNodeOutputWorld")
nt.links.new(env.outputs["Color"], bg.inputs["Color"])
nt.links.new(bg.outputs["Background"], out.inputs["Surface"])
scn.world = world

with bpy.context.temp_override(window=win, scene=scn):
    pass
bark = _tree.bark_material()
leaves = _tree.leaf_material()
leaves.surface_render_method = "DITHERED"

skel = _tree.skeleton()
crown = _tree.layout()


def link(o):
    for c in o.users_collection:
        c.objects.unlink(o)
    scn.collection.objects.link(o)
    return o


hi = link(_tree.tube_mesh(f"{P}_bark_high", skel, {0: 40, 1: 28, 2: 16, 3: 10},
                          {0: 0.025, 1: 0.04, 2: 0.05, 3: 0.05}, displace=0.07))
hi.data.materials.append(bark)
# The solid-dome canopy: one leaf-covered master dome (the bake source, at the origin and hidden),
# shown at every clump through linked duplicates (showcase only, never exported or baked).
master = link(_tree.canopy_high(f"{P}_canopy_high"))
master.data.materials.append(_tree.canopy_high_material())
master.hide_render = True
master.hide_set(True)
for i, cl in enumerate(crown):
    inst = bpy.data.objects.new(f"{P}_canopy_show_{i:02d}", master.data)
    inst.matrix_world = _tree.clump_matrix(cl)
    scn.collection.objects.link(inst)
lv = link(_tree.leaf_cards(f"{P}_leaves_high", crown))
lv.data.materials.append(leaves)
scn["tho_skeleton_sha"] = _tree.skeleton_sha(skel)

# Showcase rig (owner request): key sun, fill and rim areas. Not part of the export root.
def light(name, kind, loc, rot, energy, color=(1, 1, 1), size=None):
    d = bpy.data.lights.new(name, kind)
    d.energy = energy
    d.color = color
    if size is not None:
        d.size = size
    o = bpy.data.objects.new(name, d)
    o.location = loc
    o.rotation_euler = [math.radians(a) for a in rot]
    scn.collection.objects.link(o)
    return o


light(f"{P}_key_sun", "SUN", (8, -8, 14), (45, 0, 40), 4.0, (1.0, 0.95, 0.86)).data.angle = math.radians(2.5)
light(f"{P}_fill_area", "AREA", (-14, -10, 7), (70, 0, -55), 4000.0, (0.85, 0.9, 1.0), 8.0)
light(f"{P}_rim_area", "AREA", (-4, 16, 11), (-60, 0, 180), 6000.0, (1.0, 0.92, 0.8), 6.0)

cam_d = bpy.data.cameras.new(f"{P}_cam")
cam_d.lens = 40
cam = bpy.data.objects.new(f"{P}_cam", cam_d)
scn.collection.objects.link(cam)
cam.location = (17.0, -22.0, 5.0)  # whole crown in frame (12.5 m tree at 40 mm, as in 05_frame.py)
d = Vector((0.3, 0, 5.6)) - cam.location
cam.rotation_euler = d.to_track_quat("-Z", "Y").to_euler()
scn.camera = cam

# Showcase ground (not exported, not a bake input: the export stage reads only *_high objects).
gm = bpy.data.materials.get(f"{P}_showcase_ground") or bpy.data.materials.new(f"{P}_showcase_ground")
gp = next(n for n in gm.node_tree.nodes if n.type == "BSDF_PRINCIPLED")
gp.inputs["Base Color"].default_value = (0.09, 0.16, 0.05, 1)
gp.inputs["Roughness"].default_value = 0.95
gme = bpy.data.meshes.new(f"{P}_showcase_ground")
gme.from_pydata([(14 * math.cos(2 * math.pi * i / 64), 14 * math.sin(2 * math.pi * i / 64), 0.0) for i in range(64)],
                [], [list(range(64))])
gme.materials.append(gm)
scn.collection.objects.link(bpy.data.objects.new(f"{P}_showcase_ground", gme))

# Viewport: Rendered shading through our camera (the owner's live view of progress).
for area in win.screen.areas:
    if area.type == "VIEW_3D":
        sp = area.spaces.active
        sp.shading.type = "RENDERED"
        sp.region_3d.view_perspective = "CAMERA"

import numpy as np  # noqa: E402

mco = np.empty(len(master.data.vertices) * 3)
master.data.vertices.foreach_get("co", mco)
mco = np.c_[mco.reshape(-1, 3), np.ones(len(mco) // 3)]
pts = [np.array([v.co[:] for v in hi.data.vertices]), np.array([v.co[:] for v in lv.data.vertices])]
pts += [(mco @ np.array(_tree.clump_matrix(cl)).T)[:, :3] for cl in crown]
allp = np.concatenate(pts)
lo, hi_v = allp.min(0), allp.max(0)
print(json.dumps({"bark_high_tris": sum(len(p.vertices) - 2 for p in hi.data.polygons),
                  "canopy_master_tris": len(master.data.polygons), "fringe_cards": len(lv.data.polygons),
                  "clumps": len(crown), "branches": len(skel),
                  "bounds_min": [round(float(x), 2) for x in lo], "bounds_max": [round(float(x), 2) for x in hi_v],
                  "canopy_diam_xy": [round(float(hi_v[0] - lo[0]), 2), round(float(hi_v[1] - lo[1]), 2)],
                  "skeleton_sha": scn["tho_skeleton_sha"]}))
