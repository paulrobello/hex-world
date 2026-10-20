"""Textured headless EEVEE stills of the forest-floor props (evidence only: Blender's view transform is
not Bevy's): LOD0 of target/kit-stage/<piece>/<piece>.glb with its stage PNGs bound (albedo, with
alpha clip 0.5 on the fern's cards; ORM roughness and AO; the normal map), on a litter-brown ground,
from a three-quarter view and a low side view, plus one shot of both pieces together with a 1.7 m
figure for scale. Usage (from the repo root):
  Blender -b --factory-startup --python-exit-code 1 -P docs/checkpoints/forest_floor/still.py -- <out dir>"""
import math
import pathlib
import sys

import bpy
from mathutils import Vector

ROOT = pathlib.Path(__file__).resolve().parents[3]
out_dir = pathlib.Path(sys.argv[sys.argv.index("--") + 1])
PIECES = ("fern_clump", "log_fallen")

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


def load(piece, offset):
    """Import the piece's LOD0, bind its maps, move it by offset. Returns the object."""
    stage = ROOT / "target/kit-stage" / piece
    before = set(bpy.data.objects)
    bpy.ops.import_scene.gltf(filepath=str(stage / f"{piece}.glb"))
    new = [o for o in bpy.data.objects if o not in before]
    keep = None
    for o in new:
        if o.type == "MESH" and o.name.startswith(f"{piece}_lod0"):
            keep = o
    for o in new:
        if o is not keep and o.type == "MESH":
            bpy.data.objects.remove(o, do_unlink=True)
    keep.matrix_world = keep.matrix_world.copy()
    keep.parent = None
    keep.location += Vector(offset)
    for slot in keep.material_slots:
        mat = slot.material
        nt = mat.node_tree
        bsdf = next(n for n in nt.nodes if n.type == "BSDF_PRINCIPLED")
        alb = nt.nodes.new("ShaderNodeTexImage")
        alb.image = image(stage / f"{piece}.albedo.png", False)
        orm = nt.nodes.new("ShaderNodeTexImage")
        orm.image = image(stage / f"{piece}.orm.png", True)
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
        nrm.image = image(stage / f"{piece}.normal.png", True)
        nm = nt.nodes.new("ShaderNodeNormalMap")
        nt.links.new(nrm.outputs["Color"], nm.inputs["Color"])
        nt.links.new(nm.outputs["Normal"], bsdf.inputs["Normal"])
        if piece == "fern_clump":
            nt.links.new(alb.outputs["Alpha"], bsdf.inputs["Alpha"])
            mat.surface_render_method = "DITHERED"
            mat.use_backface_culling = False
    return keep


def shoot(name, loc, target, lens):
    cam.location = Vector(loc)
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

objs = {p: load(p, (0, 0, 0)) for p in PIECES if (ROOT / "target/kit-stage" / p / f"{p}.glb").exists()}
for p, o in objs.items():
    for q, other in objs.items():
        other.hide_render = q != p
    if p == "fern_clump":
        shoot(f"{p}-eevee-three_quarter", (1.7, -2.0, 1.5), (0, 0, 0.35), 40)
        shoot(f"{p}-eevee-low_side", (2.3, -0.4, 0.45), (0, 0, 0.35), 40)
    else:
        shoot(f"{p}-eevee-three_quarter", (3.2, -4.6, 2.6), (0, 0, 0.2), 40)
        shoot(f"{p}-eevee-low_side", (0.6, -4.2, 0.6), (0, 0, 0.25), 40)
if len(objs) == 2:
    for o in objs.values():
        o.hide_render = False
    objs["fern_clump"].location = (-1.6, 0.9, 0)
    bpy.ops.mesh.primitive_cylinder_add(radius=0.18, depth=1.7, location=(1.4, 1.3, 0.85))
    fig = bpy.context.object
    fm = bpy.data.materials.new("figure")
    next(n for n in fm.node_tree.nodes if n.type == "BSDF_PRINCIPLED").inputs["Base Color"].default_value = (0.6, 0.1, 0.08, 1)
    fig.data.materials.append(fm)
    shoot("forest_floor-eevee-together", (2.5, -6.0, 2.4), (-0.2, 0.6, 0.4), 35)
print("stills written", out_dir)
