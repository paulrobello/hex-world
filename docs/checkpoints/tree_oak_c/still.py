"""Textured headless EEVEE stills of tree_oak_c beside the other oaks (evidence only: Blender's view
transform is not Bevy's). Each oak's LOD0 with its maps bound the way the engine binds them (canopy
and leaves: the piece atlas, albedo x ORM.R, ORM.G roughness, the normal map, alpha clip 0.5 on the
leaves; bark: the tileable `bark` set on UV0 x the slot's tileUvScale, untinted). tree_oak_c comes from
target/kit-stage/tree_oak_c/ (its staged maps); tree_oak, tree_oak_b and tree_hero_oak from their
shipped GLBs and PNG maps in assets/kit/p1/. Writes tree_oak_c-eevee-three_quarter, -under (1.7 m,
under the crown), oaks-lineup (hero, tree_oak, tree_oak_b, tree_oak_c and a 1.7 m figure, level) and
oaks-elevation (an orthographic side elevation of the three scatter oaks, their silhouettes).
Usage (from the repo root):
  Blender -b --factory-startup --python-exit-code 1 -P docs/checkpoints/tree_oak_c/still.py -- <out dir>"""
import json
import math
import pathlib
import sys

import bpy
from mathutils import Vector

ROOT = pathlib.Path(__file__).resolve().parents[3]
out_dir = pathlib.Path(sys.argv[sys.argv.index("--") + 1])
BARK = ROOT / "assets/kit/p1/materials/bark"
MANIFEST = json.loads((ROOT / "assets/kit/manifest.json").read_text())
STAGE = ROOT / "target/kit-stage/tree_oak_c"

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


def bind(nt, bsdf, alb_img, orm_img, nrm_img, uv_scale=1.0, alpha=False):
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
    nt.links.new(next(x for x in mix.outputs if x.identifier == "Result_Color"), bsdf.inputs["Base Color"])
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
    """LOD0 of `piece`, textured: tree_oak_c from the stage, the others as shipped."""
    entry = next(p for p in MANIFEST["pieces"] if p["id"] == piece) if piece != "tree_oak_c" else None
    if entry is None:
        glb, maps = STAGE / f"{piece}.glb", STAGE
        tile_scale = json.loads((STAGE / "bake-report.json").read_text())["barkTile"]["tileUvScale"]
    else:
        glb, maps = ROOT / "assets/kit" / entry["glb"], ROOT / "assets/kit/p1" / piece
        tile_scale = entry["slots"][f"{piece}_bark"]["tileUvScale"]
    keep = lod0(glb, piece)
    piece_maps = [image(maps / f"{piece}.{c}.png", c != "albedo") for c in ("albedo", "orm", "normal")]
    bark_maps = [image(BARK / f"bark.{c}.png", c != "albedo") for c in ("albedo", "orm", "normal")]
    for slot in keep.material_slots:
        mat = slot.material
        nt = mat.node_tree
        bsdf = next(n for n in nt.nodes if n.type == "BSDF_PRINCIPLED")
        if mat.name.endswith("_bark"):
            bind(nt, bsdf, *bark_maps, uv_scale=tile_scale)
        else:
            leaves = mat.name.endswith("_leaves")
            bind(nt, bsdf, *piece_maps, alpha=leaves)
            if leaves:
                mat.surface_render_method = "DITHERED"
                mat.use_backface_culling = False
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


bpy.ops.mesh.primitive_plane_add(size=300.0, location=(0, 0, -0.03))
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

c = load("tree_oak_c")
h = max((c.matrix_world @ Vector(v)).z for v in c.bound_box)
shoot("tree_oak_c-eevee-three_quarter", (h * 1.6, -h * 2.0, h * 0.7), (0, 0, h * 0.45), 35)
shoot("tree_oak_c-eevee-under", (3.4, -3.8, 1.7), (0.0, 0.6, 3.3), 16)

others = {p: load(p) for p in ("tree_hero_oak", "tree_oak", "tree_oak_b")}
bpy.ops.mesh.primitive_cylinder_add(radius=0.18, depth=1.7, location=(0.0, -5.5, 0.85))
fig = bpy.context.object
fm = bpy.data.materials.new("figure")
next(n for n in fm.node_tree.nodes if n.type == "BSDF_PRINCIPLED").inputs["Base Color"].default_value = (0.6, 0.1, 0.08, 1)
fig.data.materials.append(fm)
scn.render.resolution_x, scn.render.resolution_y = 1800, 800
# Level lineup: the hero oak (left, 1.2x GLB), tree_oak, tree_oak_b, tree_oak_c (right).
others["tree_hero_oak"].location = (-21.0, 4.0, 0.0)
others["tree_oak"].location = (-7.0, 0.0, 0.0)
others["tree_oak_b"].location = (1.5, 0.0, 0.0)
c.location = (10.5, 0.0, 0.0)
fig.location = (5.2, -5.0, 0.85)
shoot("oaks-lineup", (-4.0, -46.0, 6.0), (-4.0, 0.0, 5.5), 32)
# Orthographic side elevation of the three scatter oaks (no hero): their silhouettes at one scale.
others["tree_hero_oak"].hide_render = True
fig.location = (-1.8, -3.0, 0.85)
shoot("oaks-elevation", (2.0, -60.0, 5.0), (2.0, 0.0, 5.0), 50, ortho=28.0)
print("stills written", out_dir)
