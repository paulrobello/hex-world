"""Textured headless EEVEE stills of the scatter oaks, tree_oak and tree_oak_b (evidence only: Blender's
view transform is not Bevy's): LOD0 of target/kit-stage/<piece>/<piece>.glb with its stage maps bound
the way the engine binds them (canopy and leaves: the piece atlas, albedo x ORM.R, ORM.G roughness,
the normal map, alpha clip 0.5 on the leaves; bark: the tileable `bark` set on UV0 x tileUvScale,
untinted as the manifest slot), on a litter-brown ground, from a three-quarter view each, plus both
side by side with the hero oak's staged or shipped GLB untextured for scale and a 1.7 m figure.
Usage (from the repo root):
  Blender -b --factory-startup --python-exit-code 1 -P docs/checkpoints/tree_oak/still.py -- <out dir>"""
import json
import math
import pathlib
import sys

import bpy
from mathutils import Vector

ROOT = pathlib.Path(__file__).resolve().parents[3]
out_dir = pathlib.Path(sys.argv[sys.argv.index("--") + 1])
PIECES = ("tree_oak", "tree_oak_b")
BARK = ROOT / "assets/kit/p1/materials/bark"

bpy.ops.wm.read_factory_settings(use_empty=True)
scn = bpy.context.scene
scn.render.engine = "BLENDER_EEVEE"
scn.render.resolution_x, scn.render.resolution_y = 1000, 1200
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


def bind(nt, bsdf, alb_img, orm_img, nrm_img, uv_scale=1.0, tint=None, alpha=False):
    uvn = nt.nodes.new("ShaderNodeUVMap")
    uvn.uv_map = "UVMap"
    mp = nt.nodes.new("ShaderNodeMapping")
    mp.inputs["Scale"].default_value = (uv_scale, uv_scale, 1.0)
    nt.links.new(uvn.outputs["UV"], mp.inputs["Vector"])

    def tex(img):
        n = nt.nodes.new("ShaderNodeTexImage")
        n.image = img
        nt.links.new(mp.outputs["Vector"], n.inputs["Vector"])
        return n

    alb, orm, nrm = tex(alb_img), tex(orm_img), tex(nrm_img)
    sep = nt.nodes.new("ShaderNodeSeparateColor")
    nt.links.new(orm.outputs["Color"], sep.inputs["Color"])
    mix = nt.nodes.new("ShaderNodeMix")
    mix.data_type = "RGBA"
    mix.blend_type = "MULTIPLY"
    ins = {x.identifier: x for x in mix.inputs}
    ins["Factor_Float"].default_value = 1.0
    nt.links.new(alb.outputs["Color"], ins["A_Color"])
    nt.links.new(sep.outputs["Red"], ins["B_Color"])
    col = next(x for x in mix.outputs if x.identifier == "Result_Color")
    if tint:
        t = nt.nodes.new("ShaderNodeMix")
        t.data_type = "RGBA"
        t.blend_type = "MULTIPLY"
        tin = {x.identifier: x for x in t.inputs}
        tin["Factor_Float"].default_value = 1.0
        nt.links.new(col, tin["A_Color"])
        tin["B_Color"].default_value = (*tint, 1.0)
        col = next(x for x in t.outputs if x.identifier == "Result_Color")
    nt.links.new(col, bsdf.inputs["Base Color"])
    nt.links.new(sep.outputs["Green"], bsdf.inputs["Roughness"])
    nm = nt.nodes.new("ShaderNodeNormalMap")
    nm.uv_map = "UVMap"
    nt.links.new(nrm.outputs["Color"], nm.inputs["Color"])
    nt.links.new(nm.outputs["Normal"], bsdf.inputs["Normal"])
    if alpha:
        nt.links.new(alb.outputs["Alpha"], bsdf.inputs["Alpha"])


def lod0(path, prefix):
    before = set(bpy.data.objects)
    bpy.ops.import_scene.gltf(filepath=str(path))
    new = [o for o in bpy.data.objects if o not in before]
    keep = next(o for o in new if o.type == "MESH" and o.name.startswith(f"{prefix}_lod0"))
    for o in new:
        if o is not keep and o.type == "MESH":
            bpy.data.objects.remove(o, do_unlink=True)
    keep.matrix_world = keep.matrix_world.copy()
    keep.parent = None
    return keep


def load(piece):
    stage = ROOT / "target/kit-stage" / piece
    keep = lod0(stage / f"{piece}.glb", piece)
    bake = json.loads((stage / "bake-report.json").read_text())
    tile_scale = bake["barkTile"]["tileUvScale"]
    piece_maps = [image(stage / f"{piece}.{c}.png", c != "albedo") for c in ("albedo", "orm", "normal")]
    bark_maps = [image(BARK / f"bark.{c}.png", c != "albedo") for c in ("albedo", "orm", "normal")]
    for slot in keep.material_slots:
        mat = slot.material
        nt = mat.node_tree
        bsdf = next(n for n in nt.nodes if n.type == "BSDF_PRINCIPLED")
        if mat.name.endswith("_bark"):
            bind(nt, bsdf, *bark_maps, uv_scale=tile_scale)  # untinted, as the hero oak's bark slot
        else:
            leaves = mat.name.endswith("_leaves")
            bind(nt, bsdf, *piece_maps, alpha=leaves)
            if leaves:
                mat.surface_render_method = "DITHERED"
                mat.use_backface_culling = False
    return keep


def shoot(name, loc, target, lens):
    cam.location = Vector(loc)
    cam.data.lens = lens
    cam.rotation_euler = (Vector(target) - cam.location).to_track_quat("-Z", "Y").to_euler()
    scn.render.filepath = str(out_dir / f"{name}.png")
    bpy.ops.render.render(write_still=True)


bpy.ops.mesh.primitive_plane_add(size=200.0, location=(0, 0, -0.03))
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

objs = {p: load(p) for p in PIECES if (ROOT / "target/kit-stage" / p / f"{p}.glb").exists()}
for p, o in objs.items():
    for q, other in objs.items():
        other.hide_render = q != p
    h = max((o.matrix_world @ Vector(c)).z for c in o.bound_box)
    shoot(f"{p}-eevee-three_quarter", (h * 1.15, -h * 1.45, h * 0.55), (0, 0, h * 0.5), 35)
    shoot(f"{p}-eevee-under", (h * 0.25, -h * 0.32, 1.7), (0, 0, h * 0.62), 18)
hero_glb = ROOT / "assets/kit/p1/tree_hero_oak/tree_hero_oak.glb"
if len(objs) == 2 and hero_glb.exists():
    scn.render.resolution_x, scn.render.resolution_y = 1600, 900
    for o in objs.values():
        o.hide_render = False
    objs["tree_oak"].location = (-4.0, 0.0, 0.0)
    objs["tree_oak_b"].location = (6.0, 0.0, 0.0)
    hero = lod0(hero_glb, "tree_hero_oak")  # untextured (GLB embeds no images): its size only
    hero.location = (-18.0, 4.0, 0.0)
    bpy.ops.mesh.primitive_cylinder_add(radius=0.18, depth=1.7, location=(1.2, -4.0, 0.85))
    fig = bpy.context.object
    fm = bpy.data.materials.new("figure")
    next(n for n in fm.node_tree.nodes if n.type == "BSDF_PRINCIPLED").inputs["Base Color"].default_value = (0.6, 0.1, 0.08, 1)
    fig.data.materials.append(fm)
    shoot("tree_oak-eevee-together", (-4.0, -38.0, 7.0), (-5.0, 0.0, 5.0), 30)
print("stills written", out_dir)
