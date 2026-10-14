"""Headless textured stills of the forest_forest_edge_e<k> family: a two-tile mock (two forest_base
tiles, B rotated, + their edge pieces) with a meadow tile at one corner, before and after, the corner
options O-a (shipped: straight slope on the corner ray) and O-b (mock-only taper), and one piece alone.

Run after `make kit-build PIECE=forest_forest_edge_e<k>` (k = 0, 1), the O-b mock builds
(ob_mock_build.py, k = 0, 1) and staging the bases (`HEXW_SKIP_BAKE=1 make kit-build PIECE=forest_base`
and `PIECE=meadow_base`, then `git checkout` their committed reports):
  "$BLENDER" -b --factory-startup --python-exit-code 1 -P assets-source/kit/forest_forest_edge/mock_stills.py -- <out_dir>
Writes <out_dir>/*.jpg and <out_dir>/corner-options/*.jpg. Tileable sets are mapped from world
position at tileM 2 m (as kit_material.wesl does). Blender's view transform is not Bevy's: evidence
only, the engine shot is the look reference."""
import math
import pathlib
import sys

import bpy

REPO = pathlib.Path(__file__).resolve().parents[3]
sys.path.insert(0, str(REPO / "scripts/blender"))
from lib import hexgeo  # noqa: E402

out = pathlib.Path(sys.argv[sys.argv.index("--") + 1])
(out / "corner-options").mkdir(parents=True, exist_ok=True)
STAGE = REPO / "target/kit-stage"
MATS = REPO / "assets/kit/p1/materials"
A = hexgeo.APOTHEM

bpy.ops.wm.read_factory_settings(use_empty=True)
scn = bpy.context.scene
scn.render.engine = "BLENDER_EEVEE"
scn.render.resolution_x, scn.render.resolution_y = 1600, 1000
scn.render.image_settings.file_format = "JPEG"
scn.render.image_settings.quality = 85
scn.render.use_stamp_date = False
scn.render.use_stamp_render_time = False
scn.view_settings.view_transform = "AgX"
world = bpy.data.worlds.new("mock")
scn.world = world
bg = world.node_tree.nodes["Background"]
bg.inputs[0].default_value = (0.55, 0.65, 0.8, 1.0)
bg.inputs[1].default_value = 0.6
sun = bpy.data.objects.new("sun", bpy.data.lights.new("sun", "SUN"))
sun.data.energy = 3.5
sun.data.angle = math.radians(2.0)
sun.rotation_euler = (math.radians(50), 0.0, math.radians(35))
scn.collection.objects.link(sun)


def tile_material(name, set_id, tint=(1.0, 1.0, 1.0)):
    mat = bpy.data.materials.new(name)
    nt = mat.node_tree
    bsdf = nt.nodes["Principled BSDF"]
    geo = nt.nodes.new("ShaderNodeNewGeometry")
    mapping = nt.nodes.new("ShaderNodeMapping")
    mapping.inputs["Scale"].default_value = (0.5, 0.5, 0.5)  # tileM 2 m
    nt.links.new(geo.outputs["Position"], mapping.inputs["Vector"])
    img = nt.nodes.new("ShaderNodeTexImage")
    img.image = bpy.data.images.load(str(MATS / set_id / f"{set_id}.albedo.png"))
    img.extension = "REPEAT"
    nt.links.new(mapping.outputs["Vector"], img.inputs["Vector"])
    mix = nt.nodes.new("ShaderNodeMix")
    mix.data_type = "RGBA"
    mix.blend_type = "MULTIPLY"
    mix.inputs["Factor"].default_value = 1.0
    nt.links.new(img.outputs["Color"], mix.inputs[6])
    mix.inputs[7].default_value = (*tint, 1.0)
    nt.links.new(mix.outputs[2], bsdf.inputs["Base Color"])
    bsdf.inputs["Roughness"].default_value = 0.9
    return mat


SLOT_MATS = {
    "forest_base_rim": tile_material("m_rim", "grass"),
    "forest_base_edge": tile_material("m_edge", "grass", (0.62, 0.6, 0.42)),
    "forest_base_floor": tile_material("m_floor", "litter"),
    "forest_base_skirt": tile_material("m_skirt", "soil"),
    "meadow_base_cap": tile_material("m_meadow", "grass"),
    "meadow_base_skirt": tile_material("m_mskirt", "soil"),
    "forest_forest_edge_floor": tile_material("m_ffe", "litter"),
    "forest_forest_edge_moss": tile_material("m_ffe_moss", "grass", (0.62, 0.6, 0.42)),
}


def place(piece, at=(0.0, 0.0), rot_k=0):
    """Import a staged GLB's LOD0 at hex centre `at` (Blender XY), rotated by rot_k edge steps."""
    before = set(bpy.data.objects)
    bpy.ops.import_scene.gltf(filepath=str(STAGE / piece / f"{piece}.glb"))
    new = [o for o in bpy.data.objects if o not in before]
    root = next(o for o in new if o.parent is None)
    root.rotation_mode = "XYZ"
    root.location = (at[0], at[1], 0.0)
    root.rotation_euler = (0.0, 0.0, math.radians(-60 * rot_k))
    for o in new:
        if o.type == "MESH":
            o.hide_render = "_lod0" not in o.name
            for slot in o.material_slots:
                base = slot.material.name.split(".")[0] if slot.material else ""
                if base in SLOT_MATS:
                    slot.material = SLOT_MATS[base]
    return new


def camera(name, loc, look_at, lens=35.0):
    cam = bpy.data.objects.new(name, bpy.data.cameras.new(name))
    cam.data.lens = lens
    cam.data.clip_end = 500.0
    cam.location = loc
    d = [look_at[i] - loc[i] for i in range(3)]
    cam.rotation_mode = "XYZ"
    cam.rotation_euler = (math.atan2(math.hypot(d[0], d[1]), -d[2]), 0.0, math.atan2(d[1], d[0]) - math.pi / 2)
    scn.collection.objects.link(cam)
    return cam


def shoot(cam, name):
    scn.camera = cam
    scn.render.filepath = str(out / f"{name}.jpg")
    bpy.ops.render.render(write_still=True)


def show(objs, on):
    for o in objs:
        if o.type == "MESH":
            o.hide_render = not on or "_lod0" not in o.name


B_ROT = 2  # B is rotated, so its seam edge (world edge 3) is its local edge 1: variant e1
B = (2 * A, 0.0)  # A's edge-0 neighbour
C = (A, -24.0)  # A's edge-1 neighbour (meadow): the A/B/C corner sits at (A, -8)
place("forest_base", (0.0, 0.0), 0)
place("forest_base", B, B_ROT)
place("meadow_base", C, 0)
views = {
    "mock-overview": camera("c_over", (A + 2.0, 26.0, 30.0), (A, -2.0, 0.0), 30.0),
    "mock-seam": camera("c_seam", (A - 9.0, 8.0, 7.0), (A, 0.5, 0.0), 32.0),
    "mock-corner": camera("c_corner", (A - 7.0, -1.0, 6.0), (A, -7.0, 0.0), 32.0),
}
corner = camera("c_meadow_corner", (A + 1.0, 4.0, 11.0), (A, -8.0, 0.0), 30.0)
for name, cam in views.items():
    shoot(cam, f"{name}-before")
shoot(corner, "corner-options/before")
oa = place("forest_forest_edge_e0", (0.0, 0.0), 0) + place("forest_forest_edge_e1", B, B_ROT + 1)
for name, cam in views.items():
    shoot(cam, f"{name}-after")
shoot(corner, "corner-options/O-a-after")
show(oa, False)
ob = place("forest_forest_edge_e0_ob", (0.0, 0.0), 0) + place("forest_forest_edge_e1_ob", B, B_ROT + 1)
shoot(corner, "corner-options/O-b-after")
shoot(views["mock-overview"], "corner-options/O-b-overview")
show(ob, False)

# One piece alone (A's variant e0, every base hidden), from above the seam side.
for o in bpy.data.objects:
    if o.type == "MESH":
        o.hide_render = o not in oa or "_lod0" not in o.name or "_e1" in o.name
shoot(camera("c_piece", (A - 14.0, -10.0, 14.0), (A - 2.5, 0.0, 0.0), 28.0), "piece-alone")
