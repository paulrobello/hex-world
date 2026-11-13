"""Textured headless EEVEE stills of a single-tile landform (hill_meadow, peak_granite) ringed by six
meadow_base tiles, for the owner review sheet. The material binding is massif_granite's
`review_render.py` (option o1: unique rock and scree, tileable grass and soil).

Run (from the repo root):
  "$BLENDER" -b --factory-startup --python-exit-code 1 -P docs/checkpoints/hill_meadow/review_render.py -- \
      <piece> <stage dir> <meadow_base.glb (uncompressed)> <out dir> [lod 0|1|2] \
      [shots, comma separated: play,low,close,ref,top,mask]

The stage dir is `make kit-build PIECE=<piece>`'s output (GLB plus PNG maps). The meadow GLB must
be uncompressed (the committed one is meshopt): build it with
`HEXW_SKIP_BAKE=1 "$BLENDER" -b --factory-startup --python-exit-code 1 -P scripts/blender/kit/meadow_base.py -- <dir>`.
Sun from the kit sun direction, a grey sky, AgX: evidence only, in-engine shots come from the
orchestrator. `ref` and `mask` show the piece alone (as the references do) from the front at 32
degrees; `mask` is its flat white silhouette on black.
"""
import math
import pathlib
import sys

import bpy
from mathutils import Vector

args = sys.argv[sys.argv.index("--") + 1:]
PIECE, STAGE, MEADOW_GLB, OUT = args[0], pathlib.Path(args[1]), pathlib.Path(args[2]), pathlib.Path(args[3])
LOD = int(args[4]) if len(args) > 4 else 0
SHOTS = (args[5] if len(args) > 5 else "play,low,close,ref,top").split(",")
OPTION = "o1"
REPO = pathlib.Path(__file__).resolve().parents[3]
SETS = REPO / "assets/kit/p1/materials"
APOTHEM = 16.0 * math.sqrt(3.0) / 2.0
OUT.mkdir(parents=True, exist_ok=True)

bpy.ops.wm.read_factory_settings(use_empty=True)
scn = bpy.context.scene


def load(path, non_color):
    img = bpy.data.images.load(str(path))
    img.colorspace_settings.name = "Non-Color" if non_color else "sRGB"
    return img


def import_glb(path, keep):
    before = set(bpy.data.objects)
    bpy.ops.import_scene.gltf(filepath=str(path))
    new = [o for o in bpy.data.objects if o not in before]
    for o in new:
        if o.type == "MESH" and o.name != keep:
            o.hide_render = True
    return next(o for o in new if o.name == keep)


def node(nt, kind, **inputs):
    n = nt.nodes.new(kind)
    for k, v in inputs.items():
        n.inputs[k].default_value = v
    return n


def world_tex(nt, img, tile_m):
    """Box-projected world-space sample of a tileable set (the runtime's tile frame, roughly)."""
    geo = nt.nodes.new("ShaderNodeNewGeometry")
    mp = nt.nodes.new("ShaderNodeMapping")
    mp.inputs["Scale"].default_value = (1.0 / tile_m,) * 3
    nt.links.new(geo.outputs["Position"], mp.inputs["Vector"])
    t = nt.nodes.new("ShaderNodeTexImage")
    t.image, t.projection, t.projection_blend = img, "BOX", 0.25
    nt.links.new(mp.outputs["Vector"], t.inputs["Vector"])
    return t


def uv_tex(nt, img):
    t = nt.nodes.new("ShaderNodeTexImage")
    t.image = img
    uvn = nt.nodes.new("ShaderNodeUVMap")
    uvn.uv_map = "UVMap"
    nt.links.new(uvn.outputs["UV"], t.inputs["Vector"])
    return t


def mul(nt, a, b):
    m = nt.nodes.new("ShaderNodeMix")
    m.data_type, m.blend_type = "RGBA", "MULTIPLY"
    ins = {s.identifier: s for s in m.inputs}
    ins["Factor_Float"].default_value = 1.0
    for key, x in (("A_Color", a), ("B_Color", b)):
        if isinstance(x, tuple):
            ins[key].default_value = x
        else:
            nt.links.new(x, ins[key])
    return next(s for s in m.outputs if s.identifier == "Result_Color")


def tileable(mat, set_id, tile_m, masks_img, tint=(1.0, 1.0, 1.0), rough=0.9):
    nt = mat.node_tree
    bsdf = next(n for n in nt.nodes if n.type == "BSDF_PRINCIPLED")
    alb = world_tex(nt, load(SETS / set_id / f"{set_id}.albedo.png", False), tile_m)
    col = mul(nt, alb.outputs["Color"], (*tint, 1.0))
    if masks_img is not None:
        mk = uv_tex(nt, masks_img)
        col = mul(nt, col, mk.outputs["Alpha"])
    nt.links.new(col, bsdf.inputs["Base Color"])
    bsdf.inputs["Roughness"].default_value = rough
    bsdf.inputs["Specular IOR Level"].default_value = 0.3


def unique(mat, stage, piece):
    nt = mat.node_tree
    bsdf = next(n for n in nt.nodes if n.type == "BSDF_PRINCIPLED")
    alb = uv_tex(nt, load(stage / f"{piece}.albedo.png", False))
    orm = uv_tex(nt, load(stage / f"{piece}.orm.png", True))
    sep = nt.nodes.new("ShaderNodeSeparateColor")
    nt.links.new(orm.outputs["Color"], sep.inputs["Color"])
    nt.links.new(mul(nt, alb.outputs["Color"], sep.outputs["Red"]), bsdf.inputs["Base Color"])
    nt.links.new(sep.outputs["Green"], bsdf.inputs["Roughness"])
    nm = uv_tex(nt, load(stage / f"{piece}.normal.png", True))
    nmap = nt.nodes.new("ShaderNodeNormalMap")
    nmap.uv_map = "UVMap"
    nt.links.new(nm.outputs["Color"], nmap.inputs["Color"])
    nt.links.new(nmap.outputs["Normal"], bsdf.inputs["Normal"])
    bsdf.inputs["Specular IOR Level"].default_value = 0.3


piece = import_glb(STAGE / f"{PIECE}.glb", f"{PIECE}_lod{LOD}")
meadow0 = import_glb(MEADOW_GLB, f"meadow_base_lod{LOD}")
base = meadow0.matrix_world.copy()
meadows = []
for k in range(6):
    a = math.radians(60.0 * k)  # edge normals point at 0, 60, ... degrees (hexgeo.edge_normal)
    m = meadow0 if k == 0 else meadow0.copy()
    if k:
        scn.collection.objects.link(m)
    m.parent = None
    m.matrix_world = base
    m.location = Vector((2.0 * APOTHEM * math.cos(a), 2.0 * APOTHEM * math.sin(a), 0.0)) + base.translation
    meadows.append(m)
piece_masks = load(STAGE / f"{PIECE}.masks.png", True)
meadow_masks = load(REPO / "assets/kit/p1/meadow_base/meadow_base.masks.png", True)
done = set()
for ob, masks, uniq in [(piece, piece_masks, PIECE)] + [(m, meadow_masks, None) for m in meadows]:
    for slot in ob.material_slots:
        m = slot.material
        if m.name in done:
            continue
        done.add(m.name)
        name = m.name.split(".")[0]
        if name.endswith("_cap"):
            tileable(m, "grass", 2.0, masks)
        elif name.endswith("_skirt"):
            tileable(m, "soil", 4.0, None, tint=(2.5, 2.5, 2.5), rough=0.95)
        else:
            unique(m, STAGE, uniq)

sun_dir = Vector((200.0, -150.0, 400.0)).normalized()
sun = bpy.data.objects.new("sun", bpy.data.lights.new("sun", "SUN"))
sun.data.energy, sun.data.angle = 4.0, math.radians(1.5)
sun.rotation_euler = (-sun_dir).to_track_quat("-Z", "Y").to_euler()
scn.collection.objects.link(sun)
scn.world = bpy.data.worlds.new("sky")
bg = scn.world.node_tree.nodes["Background"]
bg.inputs["Color"].default_value, bg.inputs["Strength"].default_value = (0.62, 0.66, 0.72, 1.0), 0.9
scn.render.engine = "BLENDER_EEVEE"
scn.eevee.taa_render_samples = 32
scn.view_settings.view_transform = "AgX"
scn.render.resolution_x, scn.render.resolution_y = 1600, 1000
scn.render.use_stamp_date = scn.render.use_stamp_render_time = False
scn.render.film_transparent = False
SUFFIX = "" if LOD == 0 else f"-lod{LOD}"


def shoot(name, elev_deg, azim_deg, dist, target, lens=50.0):
    cam = bpy.data.objects.new(name, bpy.data.cameras.new(name))
    cam.data.lens, cam.data.clip_end = lens, 2000.0
    e, a = math.radians(elev_deg), math.radians(azim_deg)
    t = Vector(target)
    cam.location = t + dist * Vector((math.cos(e) * math.cos(a), math.cos(e) * math.sin(a), math.sin(e)))
    cam.rotation_euler = (t - cam.location).to_track_quat("-Z", "Y").to_euler()
    scn.collection.objects.link(cam)
    scn.camera = cam
    scn.render.filepath = str(OUT / f"{PIECE}-{name}{SUFFIX}.png")
    bpy.ops.render.render(write_still=True)


def top_down(name, half=30.0):
    cam = bpy.data.objects.new(name, bpy.data.cameras.new(name))
    cam.data.type, cam.data.ortho_scale, cam.data.clip_end = "ORTHO", 2.0 * half, 500.0
    cam.location = (0.0, 0.0, 200.0)
    scn.collection.objects.link(cam)
    scn.camera = cam
    rx, ry = scn.render.resolution_x, scn.render.resolution_y
    scn.render.resolution_x = scn.render.resolution_y = 1000
    scn.render.filepath = str(OUT / f"{PIECE}-{name}{SUFFIX}.png")
    bpy.ops.render.render(write_still=True)
    scn.render.resolution_x, scn.render.resolution_y = rx, ry


H = 4.0 if PIECE == "hill_meadow" else 7.0
if "play" in SHOTS:
    shoot("play", 60.0, -90.0, 110.0, (0.0, 0.0, H * 0.5))
if "low" in SHOTS:
    shoot("low", 6.0, -70.0, 55.0, (0.0, 0.0, H))
if "close" in SHOTS:
    shoot("close", 18.0, -60.0, 30.0, (2.0, -2.0, H), lens=35.0)
for o in bpy.data.objects:
    if o.name.startswith("meadow_base"):
        o.hide_render = True
if "ref" in SHOTS:
    shoot("ref", 32.0, -90.0, 70.0, (0.0, 0.0, H * 0.6))
if "top" in SHOTS:
    top_down("top", 17.0)
if "mask" in SHOTS:
    scn.render.engine = "BLENDER_WORKBENCH"
    scn.display.shading.light, scn.display.shading.color_type = "FLAT", "SINGLE"
    scn.display.shading.single_color = (1.0, 1.0, 1.0)
    scn.view_settings.view_transform = "Standard"
    scn.world.node_tree.nodes["Background"].inputs["Color"].default_value = (0.0, 0.0, 0.0, 1.0)
    scn.display.shading.background_type = "WORLD"
    shoot("mask", 32.0, -90.0, 70.0, (0.0, 0.0, H * 0.6))
