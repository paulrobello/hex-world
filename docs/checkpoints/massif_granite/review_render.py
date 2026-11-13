"""Textured headless EEVEE stills of massif_granite beside a meadow_base, for the owner review sheet.

Run (from the repo root):
  "$BLENDER" -b --factory-startup --python-exit-code 1 -P docs/checkpoints/massif_granite/review_render.py -- \
      <massif stage dir> <meadow_base.glb (uncompressed)> <out dir> [o1|o2|o3] [lod 0|1|2] \
      [shots, comma separated: play,low,close,top,ref2,ref4,mask4,mask2] [camera dir: fit/]

The form pass's sheet uses `o1 0 play,low,close,top,ref2,ref4,mask4 docs/checkpoints/massif_granite/fit`:
ref2/ref4 render through the cameras fitted to those references (fit/cam_fit.py), top is an
orthographic top-down view, mask4 (mask2) the flat silhouettes silhouette_iou.py compares with ref-4
(ref-2).

The stage dir is `make kit-build PIECE=massif_granite`'s output (GLB plus PNG maps). The meadow
GLB must be uncompressed (the committed one is meshopt): build it with
`HEXW_SKIP_BAKE=1 "$BLENDER" -b --factory-startup --python-exit-code 1 -P scripts/blender/kit/meadow_base.py -- <dir>`.
Its masks come from the committed assets/kit/p1/meadow_base/meadow_base.masks.png.

Binding, as the runtime does it as far as EEVEE allows: unique slots take the piece albedo (times
ORM.R, the baked AO), tangent normal and ORM.G roughness on UV0; tileable slots take their set in
world space (box projection at the set's tileM, the slot tint) times masks.A on UV0. Options:
o1 the unique scree; o2 scree on the gravel set (tint 1.25); o3 the cliffs on a world-space set too
(the plaster set tinted grey, a stand-in for a granite set that does not exist). Sun from the kit
sun direction (runtime (200, 400, 150)), a grey sky, AgX. Blender's view transform is not Bevy's
tonemapper: evidence only, in-engine shots come from the orchestrator.
"""
import math
import pathlib
import sys

import bpy
from mathutils import Vector

args = sys.argv[sys.argv.index("--") + 1:]
STAGE, MEADOW_GLB, OUT = pathlib.Path(args[0]), pathlib.Path(args[1]), pathlib.Path(args[2])
OPTION = args[3] if len(args) > 3 else "o1"
LOD = int(args[4]) if len(args) > 4 else 0  # draw this LOD of both pieces (the stills' names say so)
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


massif = import_glb(STAGE / "massif_granite.glb", f"massif_granite_lod{LOD}")
meadow = import_glb(MEADOW_GLB, f"meadow_base_lod{LOD}")
# The meadow sits across outer edge 1 (ring cell 0's edge 0): the cell two steps east.
for o in bpy.data.objects:
    if o.parent is None and o.name.startswith("meadow_base"):
        o.location = (4.0 * APOTHEM, 0.0, 0.0)
massif_masks = load(STAGE / "massif_granite.masks.png", True)
meadow_masks = load(REPO / "assets/kit/p1/meadow_base/meadow_base.masks.png", True)
for ob, masks in ((massif, massif_masks), (meadow, meadow_masks)):
    for slot in ob.material_slots:
        m = slot.material
        name = m.name.split(".")[0]
        if name.endswith("_cap"):
            tileable(m, "grass", 2.0, masks)
        elif name.endswith("_skirt"):
            tileable(m, "soil", 4.0, None, tint=(2.5, 2.5, 2.5), rough=0.95)
        elif name.endswith("_scree") and OPTION in ("o2", "o3"):
            tileable(m, "gravel", 2.0, masks, tint=(1.25, 1.25, 1.25))
        elif name.endswith("_rock") and OPTION == "o3":
            tileable(m, "plaster", 2.0, masks, tint=(0.62, 0.62, 0.6), rough=0.85)
        else:
            unique(m, STAGE, "massif_granite")

# Light: the kit sun, runtime (200, 400, 150) -> Blender (200, -150, 400), and a grey sky.
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


def shoot(name, elev_deg, azim_deg, dist, target, lens=50.0):
    cam = bpy.data.objects.new(name, bpy.data.cameras.new(name))
    cam.data.lens, cam.data.clip_end = lens, 2000.0
    e, a = math.radians(elev_deg), math.radians(azim_deg)
    t = Vector(target)
    cam.location = t + dist * Vector((math.cos(e) * math.cos(a), math.cos(e) * math.sin(a), math.sin(e)))
    cam.rotation_euler = (t - cam.location).to_track_quat("-Z", "Y").to_euler()
    scn.collection.objects.link(cam)
    scn.camera = cam
    scn.render.filepath = str(OUT / f"{name}-{OPTION}{'' if LOD == 0 else f'-lod{LOD}'}.png")
    bpy.ops.render.render(write_still=True)


def fitted(name, cam_json, azim_deg, scale=0.5):
    """A camera fitted to a reference (fit/cam_fit.py: square pixels, principal point at the centre),
    turned about +Z by azim_deg from the reference's frame (its viewer at -y) into the piece's."""
    import json
    from mathutils import Matrix
    c = json.load(open(cam_json))
    rz = Matrix.Rotation(math.radians(azim_deg), 3, "Z")
    R = Matrix(c["R"])  # world-to-camera rows: right, down, forward
    right, down, fwd = Vector(R[0]), Vector(R[1]), Vector(R[2])
    m = Matrix((rz @ right, rz @ -down, rz @ -fwd)).transposed().to_4x4()
    m.translation = rz @ Vector(c["pos"])
    cam = bpy.data.objects.new(name, bpy.data.cameras.new(name))
    cam.data.sensor_fit, cam.data.sensor_width = "HORIZONTAL", 36.0
    cam.data.lens, cam.data.clip_end = c["f"] * 36.0 / c["w"], 2000.0
    cam.matrix_world = m
    scn.collection.objects.link(cam)
    scn.camera = cam
    rx, ry = scn.render.resolution_x, scn.render.resolution_y
    scn.render.resolution_x, scn.render.resolution_y = int(c["w"] * scale), int(c["h"] * scale)
    scn.render.filepath = str(OUT / f"{name}-{OPTION}{'' if LOD == 0 else f'-lod{LOD}'}.png")
    bpy.ops.render.render(write_still=True)
    scn.render.resolution_x, scn.render.resolution_y = rx, ry


def top_down(name, half=46.0):
    cam = bpy.data.objects.new(name, bpy.data.cameras.new(name))
    cam.data.type, cam.data.ortho_scale, cam.data.clip_end = "ORTHO", 2.0 * half, 500.0
    cam.location = (0.0, 0.0, 200.0)
    scn.collection.objects.link(cam)
    scn.camera = cam
    rx, ry = scn.render.resolution_x, scn.render.resolution_y
    scn.render.resolution_x = scn.render.resolution_y = 1000
    scn.render.filepath = str(OUT / f"{name}-{OPTION}.png")
    bpy.ops.render.render(write_still=True)
    scn.render.resolution_x, scn.render.resolution_y = rx, ry


SHOTS = (args[5] if len(args) > 5 else "play,low,close").split(",")
CAMS = pathlib.Path(args[6]) if len(args) > 6 else None
if "play" in SHOTS:
    # Play camera about 65 degrees, from the south (Blender -y), framing the massif and the meadow.
    shoot("play", 65.0, -90.0, 175.0, (12.0, 0.0, 4.0))
if "low" in SHOTS:
    # Low camera about 12 degrees, as ref-4.
    shoot("low", 12.0, -90.0, 120.0, (6.0, 0.0, 12.0))
if "close" in SHOTS:
    # Close-up on the cliff foot, sun side, about 25 m away.
    shoot("close", 20.0, -60.0, 32.0, (4.0, -10.0, 8.0), lens=35.0)
for o in bpy.data.objects:
    if o.name.startswith("meadow_base"):
        o.hide_render = True  # the matched and top-down views show the massif alone, as the refs do
if "top" in SHOTS:
    top_down("top")
if "ref2" in SHOTS:
    # ref-2's fitted camera (its frame is the piece's: a notch toward the viewer at -y).
    fitted("ref2cam", CAMS / "cam-ref2.json", 0.0)
if "ref4" in SHOTS:
    # ref-4's fitted camera looks along a ring cell; -30 deg turns it onto the piece's cell at -120.
    fitted("ref4cam", CAMS / "cam-ref4.json", -30.0)
if "mask4" in SHOTS or "mask2" in SHOTS:
    # Silhouettes at ref-4's camera from each of the six cell bearings (at ref-2's, from its frame
    # and the five turns of 60 degrees): flat white on black.
    scn.render.engine = "BLENDER_WORKBENCH"
    scn.display.shading.light, scn.display.shading.color_type = "FLAT", "SINGLE"
    scn.display.shading.single_color = (1.0, 1.0, 1.0)
    scn.view_settings.view_transform = "Standard"
    scn.world.node_tree.nodes["Background"].inputs["Color"].default_value = (0.0, 0.0, 0.0, 1.0)
    scn.display.shading.background_type = "WORLD"
    for k in range(6):
        if "mask4" in SHOTS:
            fitted(f"mask4-{k}", CAMS / "cam-ref4.json", -30.0 + 60.0 * k)
        if "mask2" in SHOTS:
            fitted(f"mask2-{k}", CAMS / "cam-ref2.json", 60.0 * k)
