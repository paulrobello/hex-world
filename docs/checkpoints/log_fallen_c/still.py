"""Textured headless EEVEE stills of log_fallen_c beside log_fallen and log_fallen_b (evidence only:
Blender's view transform is not Bevy's). LOD0 of target/kit-stage/<piece>/<piece>.glb with its maps
bound (albedo x ORM AO, ORM roughness, the normal map) on a litter-brown ground. The originals' maps
come from the committed assets/kit/p1/<piece>/ (their stage may be a skip-bake build), log_fallen_c's
from its stage. A lineup of the three logs with a 1.7 m figure, a top view at play-distance scale,
and log_fallen_c alone: three-quarter, into the open mouth, and low side.
Usage (from the repo root):
  Blender -b --factory-startup --python-exit-code 1 -P docs/checkpoints/log_fallen_c/still.py -- <out dir>"""
import math
import pathlib
import sys

import bpy
from mathutils import Vector

ROOT = pathlib.Path(__file__).resolve().parents[3]
out_dir = pathlib.Path(sys.argv[sys.argv.index("--") + 1])
LOGS = ("log_fallen", "log_fallen_b", "log_fallen_c")

bpy.ops.wm.read_factory_settings(use_empty=True)
scn = bpy.context.scene
scn.render.engine = "BLENDER_EEVEE"
scn.render.resolution_x, scn.render.resolution_y = 1200, 900
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


def maps_dir(piece):
    committed = ROOT / "assets/kit/p1" / piece
    if piece != "log_fallen_c" and (committed / f"{piece}.albedo.png").exists():
        return committed
    return ROOT / "target/kit-stage" / piece


def load(piece):
    """Import the piece's LOD0 from its stage GLB and bind its maps. Returns the object."""
    stage = ROOT / "target/kit-stage" / piece
    maps = maps_dir(piece)
    before = set(bpy.data.objects)
    bpy.ops.import_scene.gltf(filepath=str(stage / f"{piece}.glb"))
    new = [o for o in bpy.data.objects if o not in before]
    keep = next(o for o in new if o.type == "MESH" and o.name.startswith(f"{piece}_lod0"))
    for o in new:
        if o is not keep and o.type == "MESH":
            bpy.data.objects.remove(o, do_unlink=True)
    keep.matrix_world = keep.matrix_world.copy()
    keep.parent = None
    for slot in keep.material_slots:
        nt = slot.material.node_tree
        bsdf = next(n for n in nt.nodes if n.type == "BSDF_PRINCIPLED")
        alb = nt.nodes.new("ShaderNodeTexImage")
        alb.image = image(maps / f"{piece}.albedo.png", False)
        orm = nt.nodes.new("ShaderNodeTexImage")
        orm.image = image(maps / f"{piece}.orm.png", True)
        sep = nt.nodes.new("ShaderNodeSeparateColor")
        nt.links.new(orm.outputs["Color"], sep.inputs["Color"])
        ao = nt.nodes.new("ShaderNodeMix")
        ao.data_type = "RGBA"
        ao.blend_type = "MULTIPLY"
        ins = {x.identifier: x for x in ao.inputs}
        ins["Factor_Float"].default_value = 1.0
        nt.links.new(alb.outputs["Color"], ins["A_Color"])
        nt.links.new(sep.outputs["Red"], ins["B_Color"])
        nt.links.new(next(x for x in ao.outputs if x.identifier == "Result_Color"), bsdf.inputs["Base Color"])
        nt.links.new(sep.outputs["Green"], bsdf.inputs["Roughness"])
        nrm = nt.nodes.new("ShaderNodeTexImage")
        nrm.image = image(maps / f"{piece}.normal.png", True)
        nm = nt.nodes.new("ShaderNodeNormalMap")
        nt.links.new(nrm.outputs["Color"], nm.inputs["Color"])
        nt.links.new(nm.outputs["Normal"], bsdf.inputs["Normal"])
    return keep


def shoot(name, loc, target, lens, ortho=None):
    cam.location = Vector(loc)
    cam.data.type = "ORTHO" if ortho else "PERSP"
    if ortho:
        cam.data.ortho_scale = ortho
    cam.data.lens = lens
    cam.rotation_euler = (Vector(target) - cam.location).to_track_quat("-Z", "Y").to_euler()
    scn.render.filepath = str(out_dir / f"{name}.png")
    bpy.ops.render.render(write_still=True)


bpy.ops.mesh.primitive_plane_add(size=80.0, location=(0, 0, -0.03))
g = bpy.context.object
gm = bpy.data.materials.new("ground")
next(n for n in gm.node_tree.nodes if n.type == "BSDF_PRINCIPLED").inputs["Base Color"].default_value = (0.11, 0.07, 0.035, 1)
g.data.materials.append(gm)
sun = bpy.data.objects.new("sun", bpy.data.lights.new("sun", "SUN"))
sun.data.energy = 4.0
sun.rotation_euler = (math.radians(48), 0, math.radians(35))
scn.collection.objects.link(sun)
cam = bpy.data.objects.new("cam", bpy.data.cameras.new("cam"))
scn.collection.objects.link(cam)
scn.camera = cam
bpy.ops.mesh.primitive_cylinder_add(radius=0.18, depth=1.7, location=(0, 0, 0.85))
fig = bpy.context.object
fm = bpy.data.materials.new("figure")
next(n for n in fm.node_tree.nodes if n.type == "BSDF_PRINCIPLED").inputs["Base Color"].default_value = (0.6, 0.1, 0.08, 1)
fig.data.materials.append(fm)

objs = {p: load(p) for p in LOGS}


def show(only):
    for p, o in objs.items():
        o.hide_render = p not in only


# The lineup: log_fallen at the back, log_fallen_b in the middle, log_fallen_c in front; the figure
# at the right.
for o, y in zip((objs[p] for p in LOGS), (2.0, 0.6, -0.9)):
    o.location = (0, y, 0)
fig.location = (2.8, 0.6, 0.85)
show(LOGS)
shoot("logs-lineup", (3.4, -6.4, 2.9), (0.4, 0.4, 0.2), 35)
# Top view, ortho over 9 m at 1200 px wide (about 130 px/m; play distance is far less), so the plan
# shapes compare directly.
fig.hide_render = True
shoot("logs-top", (0, 0.5, 30.0), (0, 0.5, 0), 50, ortho=8.0)
# log_fallen_c alone at the pivot.
objs["log_fallen_c"].location = (0, 0, 0)
show(("log_fallen_c",))
shoot("log_fallen_c-eevee-three_quarter", (2.6, -3.4, 2.1), (0, 0, 0.25), 40)
shoot("log_fallen_c-eevee-mouth", (3.1, -0.9, 0.9), (1.4, 0, 0.3), 40)
shoot("log_fallen_c-eevee-low_side", (0.3, -3.6, 0.45), (0, 0, 0.25), 40)
print("stills written", out_dir)
