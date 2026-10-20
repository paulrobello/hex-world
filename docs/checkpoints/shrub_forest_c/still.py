"""Textured headless EEVEE stills of shrub_forest_c (evidence only: Blender's view transform is not
Bevy's). LOD0 of target/kit-stage/shrub_forest_c/shrub_forest_c.glb with its maps bound (albedo with
alpha clip 0.5, ORM roughness and AO, the normal map) on a litter-brown ground, in a lineup with the
committed shrub_forest, shrub_forest_b, sapling_spruce and fern_clump (assets/kit/p1) and a 1.7 m
figure; then a three-quarter, a low side, a top view and a close-up of shrub_forest_c. Every slot with
a manifest `alphaCutoff` binds its piece's maps; colour-only slots take the manifest colour, and a
tileable (bark) slot a plain bark brown.
Usage (from the repo root):
  Blender -b --factory-startup --python-exit-code 1 -P docs/checkpoints/shrub_forest_c/still.py -- <out dir>"""
import json
import math
import pathlib
import sys

import bpy
from mathutils import Vector

ROOT = pathlib.Path(__file__).resolve().parents[3]
out_dir = pathlib.Path(sys.argv[sys.argv.index("--") + 1])
NEW = ("shrub_forest_c",)
OLD = ("shrub_forest", "shrub_forest_b", "sapling_spruce", "fern_clump")
MANIFEST = {p["id"]: p for p in json.loads((ROOT / "assets/kit/manifest.json").read_text())["pieces"]}
# shrub_forest_c's slots, as its manifest entry has them (the piece may be staged before it is listed).
NEW_SLOTS = {"shrub_forest_c_leaves": {"alphaCutoff": 0.5}, "shrub_forest_c_stems": {"color": [0.11, 0.045, 0.04]}}

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


def principled(mat):
    return next(n for n in mat.node_tree.nodes if n.type == "BSDF_PRINCIPLED")


def bind(mat, maps, piece):
    nt = mat.node_tree
    bsdf = principled(mat)
    alb = nt.nodes.new("ShaderNodeTexImage")
    alb.image = image(maps / f"{piece}.albedo.png", False)
    orm = nt.nodes.new("ShaderNodeTexImage")
    orm.image = image(maps / f"{piece}.orm.png", True)
    nrm = nt.nodes.new("ShaderNodeTexImage")
    nrm.image = image(maps / f"{piece}.normal.png", True)
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
    nm = nt.nodes.new("ShaderNodeNormalMap")
    nt.links.new(nrm.outputs["Color"], nm.inputs["Color"])
    nt.links.new(nm.outputs["Normal"], bsdf.inputs["Normal"])
    nt.links.new(alb.outputs["Alpha"], bsdf.inputs["Alpha"])
    mat.surface_render_method = "DITHERED"
    mat.use_backface_culling = False


def load(piece):
    src = ROOT / "assets/kit/p1" / piece if piece in OLD else ROOT / "target/kit-stage" / piece
    slots = NEW_SLOTS if piece in NEW else MANIFEST[piece]["slots"]
    before = set(bpy.data.objects)
    bpy.ops.import_scene.gltf(filepath=str(src / f"{piece}.glb"))
    new = [o for o in bpy.data.objects if o not in before]
    keep = next(o for o in new if o.type == "MESH" and o.name.startswith(f"{piece}_lod0"))
    for o in new:
        if o is not keep and o.type == "MESH":
            bpy.data.objects.remove(o, do_unlink=True)
    keep.matrix_world = keep.matrix_world.copy()
    keep.parent = None
    for slot in keep.material_slots:
        mat = slot.material
        info = slots.get(mat.name, {})
        if "alphaCutoff" in info:
            bind(mat, src, piece)
        elif "tileable" in info:
            principled(mat).inputs["Base Color"].default_value = (0.12, 0.08, 0.05, 1)
        else:
            principled(mat).inputs["Base Color"].default_value = (*info.get("color", (0.1, 0.075, 0.05)), 1)
    return keep


def flat(name, size, loc, colour):
    bpy.ops.mesh.primitive_plane_add(size=1.0, location=loc)
    o = bpy.context.object
    o.scale = (*size, 1)
    m = bpy.data.materials.new(name)
    principled(m).inputs["Base Color"].default_value = (*colour, 1)
    principled(m).inputs["Roughness"].default_value = 0.9
    o.data.materials.append(m)
    return o


def shoot(name, loc, target, lens):
    cam.location = Vector(loc)
    cam.data.lens = lens
    cam.rotation_euler = (Vector(target) - cam.location).to_track_quat("-Z", "Y").to_euler()
    scn.render.filepath = str(out_dir / f"{name}.png")
    bpy.ops.render.render(write_still=True)


flat("litter", (60.0, 60.0), (0, 0, -0.02), (0.12, 0.07, 0.035))
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
principled(fm).inputs["Base Color"].default_value = (0.6, 0.1, 0.08, 1)
fig.data.materials.append(fm)

objs = {p: load(p) for p in (*OLD, *NEW)}


def show(only):
    for p, o in objs.items():
        o.hide_render = p not in only
    fig.hide_render = "fig" not in only


# Lineup: shrub_forest, shrub_forest_b, shrub_forest_c, sapling_spruce, fern_clump, the figure.
xs = {"shrub_forest": -4.6, "shrub_forest_b": -2.3, "shrub_forest_c": 0.2, "sapling_spruce": 2.6, "fern_clump": 4.2}
for p, o in objs.items():
    o.location = (xs[p], 0, 0)
fig.location = (5.6, 0.3, 0.85)
show((*OLD, *NEW, "fig"))
shoot("shrubc-lineup", (0.4, -12.0, 2.4), (0.4, 0.5, 0.85), 30)
for p in NEW:
    objs[p].location = (0, 0, 0)
    show((p,))
    shoot(f"{p}-eevee-three_quarter", (3.2, -4.2, 2.1), (0, 0, 0.6), 40)
    shoot(f"{p}-eevee-low_side", (4.6, -1.0, 0.5), (0, 0, 0.6), 40)
    shoot(f"{p}-eevee-top", (0.4, -0.6, 5.4), (0, 0, 0.5), 40)
    shoot(f"{p}-eevee-close", (1.4, -1.6, 1.4), (0.3, 0.1, 0.8), 40)
print("stills written", out_dir, OLD, NEW)
