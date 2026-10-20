"""Textured headless EEVEE stills of tree_hero_oak_c beside tree_hero_oak and tree_hero_oak_b (evidence
only: Blender's view transform is not Bevy's). Each LOD0 has its maps bound as the engine binds them
(canopy and leaves: the piece atlas, albedo x ORM.R, ORM.G roughness, normal map, alpha clip on the
leaves; bark: the tileable `bark` set on UV0 x tileUvScale 32.4). The hero and _b load their shipped
GLBs and PNGs from assets/kit/p1, _c its stage GLB and maps (target/kit-stage). Writes into <out dir>:
  c-lineup.jpg   hero, _b, _c left to right from the south (the tile's default camera side)
  c-lineup-e.jpg the same three from the east
  c-close.jpg    _c's dead top and stub from the south-east
  c-wind-u.jpg   _c's LOD0 coloured by wind U (TEXCOORD_1 sway: black 0 at the base, white 1)
Usage (from the repo root):
  Blender -b --factory-startup --python-exit-code 1 -P docs/checkpoints/tree_hero_oak_c/still.py -- <out dir>"""
import math
import pathlib
import subprocess
import sys

import bpy
from mathutils import Vector

ROOT = pathlib.Path(__file__).resolve().parents[3]
out_dir = pathlib.Path(sys.argv[sys.argv.index("--") + 1])
out_dir.mkdir(parents=True, exist_ok=True)
BARK = ROOT / "assets/kit/p1/materials/bark"
TILE = 32.4

bpy.ops.wm.read_factory_settings(use_empty=True)
scn = bpy.context.scene
scn.render.engine = "BLENDER_EEVEE"
scn.render.resolution_x, scn.render.resolution_y = 1600, 900
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


def load(piece, glb, maps_dir):
    before = set(bpy.data.objects)
    bpy.ops.import_scene.gltf(filepath=str(glb))
    new = [o for o in bpy.data.objects if o not in before]
    keep = next(o for o in new if o.type == "MESH" and o.name.startswith(f"{piece}_lod0"))
    for o in new:
        if o is not keep and o.type == "MESH":
            bpy.data.objects.remove(o, do_unlink=True)
    keep.matrix_world = keep.matrix_world.copy()
    keep.parent = None
    piece_maps = [image(maps_dir / f"{piece}.{c}.png", c != "albedo") for c in ("albedo", "orm", "normal")]
    bark_maps = [image(BARK / f"bark.{c}.png", c != "albedo") for c in ("albedo", "orm", "normal")]
    for slot in keep.material_slots:
        mat = slot.material
        nt = mat.node_tree
        bsdf = next(n for n in nt.nodes if n.type == "BSDF_PRINCIPLED")
        if mat.name.endswith("_bark"):
            bind(nt, bsdf, *bark_maps, uv_scale=TILE)
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
    png = out_dir / f"{name}.png"
    scn.render.filepath = str(png)
    bpy.ops.render.render(write_still=True)
    subprocess.run(["ffmpeg", "-loglevel", "error", "-y", "-i", str(png), "-q:v", "2", str(png.with_suffix(".jpg"))],
                   check=True)
    png.unlink()


bpy.ops.mesh.primitive_plane_add(size=400.0, location=(0, 0, -0.03))
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

P1 = ROOT / "assets/kit/p1"
STAGE = ROOT / "target/kit-stage/tree_hero_oak_c"
hero = load("tree_hero_oak", P1 / "tree_hero_oak/tree_hero_oak.glb", P1 / "tree_hero_oak")
var_b = load("tree_hero_oak_b", P1 / "tree_hero_oak_b/tree_hero_oak_b.glb", P1 / "tree_hero_oak_b")
var = load("tree_hero_oak_c", STAGE / "tree_hero_oak_c.glb", STAGE)
for o, x in ((hero, -19.0), (var_b, 0.0), (var, 19.0)):
    o.location = (x, 0.0, 0.0)
shoot("c-lineup", (0.0, -78.0, 16.0), (0.0, 0.0, 6.0), 30)
for o, y in ((hero, 19.0), (var_b, 0.0), (var, -19.0)):
    o.location = (0.0, y, 0.0)
shoot("c-lineup-e", (78.0, 0.0, 16.0), (0.0, 0.0, 6.0), 30)
for o, x in ((hero, -19.0), (var_b, 0.0), (var, 19.0)):
    o.location = (x, 0.0, 0.0)
shoot("c-lineup-top", (0.0, -0.01, 95.0), (0.0, 0.0, 0.0), 30)
hero.hide_render = var_b.hide_render = True
var.location = (0.0, 0.0, 0.0)
shoot("c-close", (15.0, -17.0, 6.0), (0.4, 0.0, 6.5), 24)

# Wind U as an emission colour on every slot of the variant. The glTF importer names TEXCOORD_1 after
# its own scheme, so take the second UV layer by index.
wind_uv = var.data.uv_layers[1].name
for slot in var.material_slots:
    mat = slot.material
    nt = mat.node_tree
    for n in list(nt.nodes):
        if n.type != "OUTPUT_MATERIAL":
            nt.nodes.remove(n)
    out = next(n for n in nt.nodes if n.type == "OUTPUT_MATERIAL")
    uvn = nt.nodes.new("ShaderNodeUVMap")
    uvn.uv_map = wind_uv
    sep = nt.nodes.new("ShaderNodeSeparateXYZ")
    nt.links.new(uvn.outputs["UV"], sep.inputs[0])
    em = nt.nodes.new("ShaderNodeEmission")
    nt.links.new(sep.outputs["X"], em.inputs["Color"])
    nt.links.new(em.outputs["Emission"], out.inputs["Surface"])
    mat.surface_render_method = "DITHERED"
scn.view_settings.view_transform = "Standard"
g.hide_render = True
shoot("c-wind-u", (0.0, -40.0, 8.0), (0.0, 0.0, 6.0), 30)
print("stills written", out_dir)
