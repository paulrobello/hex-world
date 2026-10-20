"""Textured headless EEVEE still of wheat_field (evidence only: Blender's view transform is not Bevy's):
LOD0 from a GLB with the piece's PNG maps bound on the wheat slot (albedo with alpha clip 0.5, ORM
roughness and AO, the normal map) and the earth slot drawn as the runtime does, the world-space
`gravel` set (2 m tile) times the manifest tint, on a meadow-green ground, from the play side
(three-quarter) and along the rows at eye level.
Usage (from the repo root):
  Blender -b --factory-startup --python-exit-code 1 -P docs/checkpoints/wheat_field/still.py -- \
    <glb> <maps dir> <out dir>"""
import json
import math
import pathlib
import sys

import bpy
from mathutils import Vector

ROOT = pathlib.Path(__file__).resolve().parents[3]
glb, maps, out_dir = sys.argv[sys.argv.index("--") + 1:]
maps = pathlib.Path(maps)
P = "wheat_field"
manifest = json.loads((ROOT / "assets/kit/manifest.json").read_text())
entry = next((p for p in manifest["pieces"] if p["id"] == P), None)
SOIL_TINT = tuple(entry["slots"][f"{P}_soil"]["color"]) if entry else (0.40, 0.27, 0.20)

bpy.ops.wm.read_factory_settings(use_empty=True)
scn = bpy.context.scene
scn.render.engine = "BLENDER_EEVEE"
scn.render.resolution_x, scn.render.resolution_y = 1400, 900
scn.render.use_stamp_date = False
scn.render.use_stamp_render_time = False
scn.view_settings.view_transform = "AgX"
world = bpy.data.worlds.new("still")
world.color = (0.5, 0.6, 0.75)
scn.world = world


def image(path, non_color):
    img = bpy.data.images.load(str(path))
    img.colorspace_settings.name = "Non-Color" if non_color else "sRGB"
    return img


bpy.ops.import_scene.gltf(filepath=str(glb))
for o in [o for o in bpy.data.objects if o.type == "MESH" and not o.name.startswith(f"{P}_lod0")]:
    bpy.data.objects.remove(o, do_unlink=True)
body = next(o for o in bpy.data.objects if o.type == "MESH")
for slot in body.material_slots:
    mat = slot.material
    nt = mat.node_tree
    bsdf = next(n for n in nt.nodes if n.type == "BSDF_PRINCIPLED")
    if mat.name.endswith("_wheat"):
        alb = nt.nodes.new("ShaderNodeTexImage")
        alb.image = image(maps / f"{P}.albedo.png", False)
        orm = nt.nodes.new("ShaderNodeTexImage")
        orm.image = image(maps / f"{P}.orm.png", True)
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
        nt.links.new(alb.outputs["Alpha"], bsdf.inputs["Alpha"])
        nrm = nt.nodes.new("ShaderNodeTexImage")
        nrm.image = image(maps / f"{P}.normal.png", True)
        nm = nt.nodes.new("ShaderNodeNormalMap")
        nt.links.new(nrm.outputs["Color"], nm.inputs["Color"])
        nt.links.new(nm.outputs["Normal"], bsdf.inputs["Normal"])
        mat.blend_method = "CLIP" if hasattr(mat, "blend_method") else None
        mat.surface_render_method = "DITHERED"
        mat.use_backface_culling = False
    else:
        gdir = ROOT / "assets/kit/p1/materials/gravel"
        coord = nt.nodes.new("ShaderNodeTexCoord")
        mp = nt.nodes.new("ShaderNodeMapping")
        mp.inputs["Scale"].default_value = (0.5, 0.5, 0.5)  # 2 m per tile
        nt.links.new(coord.outputs["Object"], mp.inputs["Vector"])
        alb = nt.nodes.new("ShaderNodeTexImage")
        alb.image = image(gdir / "gravel.albedo.png", False)
        alb.projection = "BOX"
        nt.links.new(mp.outputs["Vector"], alb.inputs["Vector"])
        tint = nt.nodes.new("ShaderNodeMix")
        tint.data_type = "RGBA"
        tint.blend_type = "MULTIPLY"
        ins = {x.identifier: x for x in tint.inputs}
        ins["Factor_Float"].default_value = 1.0
        ins["B_Color"].default_value = (*SOIL_TINT, 1.0)
        nt.links.new(alb.outputs["Color"], ins["A_Color"])
        nt.links.new(next(x for x in tint.outputs if x.identifier == "Result_Color"), bsdf.inputs["Base Color"])
        bsdf.inputs["Roughness"].default_value = 0.95

bpy.ops.mesh.primitive_plane_add(size=60.0, location=(0, 0, -0.12))
g = bpy.context.object
gm = bpy.data.materials.new("ground")
next(n for n in gm.node_tree.nodes if n.type == "BSDF_PRINCIPLED").inputs["Base Color"].default_value = (0.12, 0.24, 0.05, 1)
g.data.materials.append(gm)
sun = bpy.data.objects.new("sun", bpy.data.lights.new("sun", "SUN"))
sun.data.energy = 4.0
sun.rotation_euler = (math.radians(50), 0, math.radians(30))
scn.collection.objects.link(sun)
cam = bpy.data.objects.new("cam", bpy.data.cameras.new("cam"))
scn.collection.objects.link(cam)
scn.camera = cam
# Blender -y is runtime +z, the play camera's side; the cards face -y.
for name, loc, target, lens in (
    ("play", Vector((4.0, -34.0, 26.0)), Vector((0.0, 0.0, 0.0)), 50),
    ("close", Vector((3.0, -14.5, 2.2)), Vector((0.5, -6.0, 0.5)), 35),
    ("rows", Vector((-13.5, -2.0, 1.7)), Vector((0.0, 0.5, 0.6)), 30),
):
    cam.location = loc
    cam.data.lens = lens
    cam.rotation_euler = (target - cam.location).to_track_quat("-Z", "Y").to_euler()
    scn.render.filepath = str(pathlib.Path(out_dir) / f"{P}-eevee-{name}.png")
    bpy.ops.render.render(write_still=True)
print("stills written", out_dir)
