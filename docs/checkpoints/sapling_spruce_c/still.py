"""Textured headless EEVEE stills of sapling_spruce_c (evidence only: Blender's view transform is not
Bevy's), copied from docs/checkpoints/sapling_spruce_b/still.py: LOD0 of target/kit-stage/<piece>/<piece>.glb
with its maps bound (albedo with alpha clip 0.5 on alpha-card slots, ORM roughness and AO, the normal
map; the spruce bark slot takes the committed `bark` set at its tileUvScale), on a litter-brown
ground. Maps of the neighbours (sapling_spruce, sapling_spruce_b, tree_forest_c) come from
assets/kit/p1, the new piece's from the stage. A lineup with tree_forest_c, both sibling saplings and
a 1.7 m figure for scale, then a three-quarter, a low side and a top view of the new piece.
Usage (from the repo root; the new piece staged with make kit-build PIECE=sapling_spruce_c, the
neighbours with HEXW_SKIP_BAKE=1 Blender -b ... -P scripts/blender/kit/<id>.py -- target/kit-stage/<id>,
which leaves their committed reports alone):
  Blender -b --factory-startup --python-exit-code 1 -P docs/checkpoints/sapling_spruce_c/still.py -- <out dir>"""
import json
import math
import pathlib
import sys

import bpy
from mathutils import Vector

ROOT = pathlib.Path(__file__).resolve().parents[3]
out_dir = pathlib.Path(sys.argv[sys.argv.index("--") + 1])
NEW = ["sapling_spruce_c"]
OLD = ("tree_forest_c", "sapling_spruce", "sapling_spruce_b")
MANIFEST = {p["id"]: p for p in json.loads((ROOT / "assets/kit/manifest.json").read_text())["pieces"]}

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
    return ROOT / "assets/kit/p1" / piece if piece in OLD else ROOT / "target/kit-stage" / piece


def slot_info(piece, slot):
    """The manifest slot, or the stage's proposed one for a piece not yet in the manifest."""
    if piece in MANIFEST:
        return MANIFEST[piece]["slots"].get(slot, {})
    prop = ROOT / "target/kit-stage" / piece / "manifest-entry.json"
    if prop.exists():
        return json.loads(prop.read_text())["slots"].get(slot, {})
    if slot.endswith("_bark"):
        return {"tileable": "bark", "tileUvScale": 32.0, "color": [0.92, 0.8, 0.72]}
    if slot.endswith(("_core", "_stems")):
        return {"color": [0.03, 0.055, 0.024]}
    return {"unique": True, "alphaCutoff": 0.5}


def bind(nt, bsdf, alb_path, orm_path, nrm_path, uv_scale=None, tint=None, alpha=False, mat=None):
    uvn = None
    if uv_scale:
        tc = nt.nodes.new("ShaderNodeTexCoord")
        mp = nt.nodes.new("ShaderNodeMapping")
        mp.inputs["Scale"].default_value = (uv_scale, uv_scale, 1)
        nt.links.new(tc.outputs["UV"], mp.inputs["Vector"])
        uvn = mp.outputs["Vector"]
    alb = nt.nodes.new("ShaderNodeTexImage")
    alb.image = image(alb_path, False)
    orm = nt.nodes.new("ShaderNodeTexImage")
    orm.image = image(orm_path, True)
    nrm = nt.nodes.new("ShaderNodeTexImage")
    nrm.image = image(nrm_path, True)
    if uvn is not None:
        for t in (alb, orm, nrm):
            nt.links.new(uvn, t.inputs["Vector"])
    sep = nt.nodes.new("ShaderNodeSeparateColor")
    nt.links.new(orm.outputs["Color"], sep.inputs["Color"])
    ao = nt.nodes.new("ShaderNodeMix")
    ao.data_type = "RGBA"
    ao.blend_type = "MULTIPLY"
    ins = {x.identifier: x for x in ao.inputs}
    ins["Factor_Float"].default_value = 1.0
    col = alb.outputs["Color"]
    if tint:
        tn = nt.nodes.new("ShaderNodeMix")
        tn.data_type = "RGBA"
        tn.blend_type = "MULTIPLY"
        ti = {x.identifier: x for x in tn.inputs}
        ti["Factor_Float"].default_value = 1.0
        ti["B_Color"].default_value = (*tint, 1)
        nt.links.new(col, ti["A_Color"])
        col = next(x for x in tn.outputs if x.identifier == "Result_Color")
    nt.links.new(col, ins["A_Color"])
    nt.links.new(sep.outputs["Red"], ins["B_Color"])
    nt.links.new(next(x for x in ao.outputs if x.identifier == "Result_Color"), bsdf.inputs["Base Color"])
    nt.links.new(sep.outputs["Green"], bsdf.inputs["Roughness"])
    nm = nt.nodes.new("ShaderNodeNormalMap")
    nt.links.new(nrm.outputs["Color"], nm.inputs["Color"])
    nt.links.new(nm.outputs["Normal"], bsdf.inputs["Normal"])
    if alpha:
        nt.links.new(alb.outputs["Alpha"], bsdf.inputs["Alpha"])
        mat.surface_render_method = "DITHERED"
        mat.use_backface_culling = False


def load(piece):
    """Import the piece's LOD0 from its stage GLB and bind its maps per slot. Returns the object."""
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
    bark = ROOT / "assets/kit/p1/materials/bark/bark"
    for slot in keep.material_slots:
        mat = slot.material
        nt = mat.node_tree
        bsdf = next(n for n in nt.nodes if n.type == "BSDF_PRINCIPLED")
        info = slot_info(piece, mat.name)
        if info.get("tileable") == "bark":
            bind(nt, bsdf, f"{bark}.albedo.png", f"{bark}.orm.png", f"{bark}.normal.png",
                 uv_scale=info["tileUvScale"], tint=info.get("color"))
        elif info.get("unique"):
            bind(nt, bsdf, maps / f"{piece}.albedo.png", maps / f"{piece}.orm.png", maps / f"{piece}.normal.png",
                 alpha="alphaCutoff" in info, mat=mat)
        else:
            c = info.get("color", (0.05, 0.05, 0.05))
            bsdf.inputs["Base Color"].default_value = (*c, 1)
            bsdf.inputs["Roughness"].default_value = info.get("roughness", 0.9)
    return keep


def shoot(name, loc, target, lens):
    cam.location = Vector(loc)
    cam.data.lens = lens
    cam.rotation_euler = (Vector(target) - cam.location).to_track_quat("-Z", "Y").to_euler()
    scn.render.filepath = str(out_dir / f"{name}.png")
    bpy.ops.render.render(write_still=True)


bpy.ops.mesh.primitive_plane_add(size=120.0, location=(0, 0, -0.03))
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

objs = {p: load(p) for p in (*OLD, *NEW)}


def show(only):
    for p, o in objs.items():
        o.hide_render = p not in only
    fig.hide_render = "fig" not in only


# Lineup: tree_forest_c at the back, then sapling_spruce, sapling_spruce_b, the new piece and the
# 1.7 m figure in a row.
objs["tree_forest_c"].location = (0.0, 6.0, 0)
row = ["sapling_spruce", "sapling_spruce_b", *NEW]
for p, x in zip(row, (-2.4, 0.0, 2.4)):
    objs[p].location = (x, 0, 0)
fig.location = (4.1, 0.3, 0.85)
show((*OLD, *NEW, "fig"))
shoot("saplingc-lineup", (0.8, -9.5, 1.9), (0.8, 1.5, 1.35), 30)
shoot("saplingc-lineup-wide", (0.4, -16.0, 5.0), (0.2, 3.0, 5.5), 30)
for p in NEW:
    objs[p].location = (0, 0, 0)
    show((p,))
    shoot(f"{p}-eevee-three_quarter", (2.6, -3.2, 2.1), (0, 0, 0.9), 40)
    shoot(f"{p}-eevee-low_side", (3.4, -0.8, 0.6), (0, 0, 0.9), 40)
    shoot(f"{p}-eevee-top", (0.4, -0.6, 4.6), (0, 0, 0.5), 40)
print("stills written", out_dir, NEW)
