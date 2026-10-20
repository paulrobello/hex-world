"""Headless textured EEVEE stills of the staged house_cottage_b (the six-view render is untextured),
alone and beside the committed house_cottage and house_longhouse (the other two designs of the cottage
slot) at true scale.

Run after `make kit-build PIECE=house_cottage_b`:
  "$BLENDER" -b --factory-startup --python-exit-code 1 -P docs/checkpoints/house_cottage_b/eevee_still.py -- <out_dir> [lod]
Writes <out_dir>/eevee-{three-quarter,front,back,top,night,trio}.jpg (LOD1/LOD2: only the first three,
suffixed). Tileable slots sample the plaster set box-projected from world position at tileM 2 m,
tinted by the manifest colour (as kit_material.wesl does). Colour slots take the manifest colour. The
masks map is bound on UV0: A darkens the base colour by 60% (the runtime applies it to ambient light
only, so this overstates it in direct sun) and B x 4 is the night emission. Blender's view transform
is not Bevy's: evidence only. From docs/checkpoints/bakery_shop/eevee_still.py."""
import json
import math
import pathlib
import sys

import bpy

REPO = pathlib.Path(__file__).resolve().parents[3]
P = "house_cottage_b"
args = sys.argv[sys.argv.index("--") + 1:]
out = pathlib.Path(args[0])
lod = int(args[1]) if len(args) > 1 else 0
out.mkdir(parents=True, exist_ok=True)
STAGE = REPO / "target/kit-stage" / P
KIT = REPO / "assets/kit"
MATS = KIT / "p1/materials"
MANIFEST = json.loads((KIT / "manifest.json").read_text())
PIECES = {p["id"]: p for p in MANIFEST["pieces"]}
# This piece's manifest tints (it may not be in the manifest yet when the still runs).
OWN_TILE = {"walls": (0.98, 0.94, 0.84), "roof": (0.105, 0.11, 0.125), "stone": (0.48, 0.45, 0.4),
            "pots": (0.5, 0.19, 0.095)}

bpy.ops.wm.read_factory_settings(use_empty=True)
scn = bpy.context.scene
scn.render.engine = "BLENDER_EEVEE"
scn.render.resolution_x, scn.render.resolution_y = 1400, 1050
scn.render.image_settings.file_format = "JPEG"
scn.render.image_settings.quality = 85
scn.render.use_stamp_date = False
scn.render.use_stamp_render_time = False
scn.view_settings.view_transform = "AgX"
world = bpy.data.worlds.new("still")
scn.world = world
bg = world.node_tree.nodes["Background"]
bg.inputs[0].default_value = (0.5, 0.52, 0.55, 1.0)
bg.inputs[1].default_value = 0.7
sun = bpy.data.objects.new("sun", bpy.data.lights.new("sun", "SUN"))
sun.data.energy = 3.5
sun.data.angle = math.radians(2.0)
sun.rotation_euler = (math.radians(40), 0.0, math.radians(120))
scn.collection.objects.link(sun)
plaster = bpy.data.images.load(str(MATS / "plaster/plaster.albedo.png"))


def masks_image(path):
    if path.exists():
        img = bpy.data.images.load(str(path))
    else:  # a HEXW_SKIP_BAKE=1 stage: neutral masks (no AO, no night)
        img = bpy.data.images.new("masks", 4, 4, alpha=True)
        img.pixels.foreach_set([1.0, 0.5, 0.0, 1.0] * 16)
    img.colorspace_settings.name = "Non-Color"
    return img


def material(piece, slot, src, masks_img, tile):
    mat = bpy.data.materials.new(f"still_{slot}")
    nt = mat.node_tree
    bsdf = nt.nodes["Principled BSDF"]
    sb = src.node_tree.nodes["Principled BSDF"] if src and src.node_tree else None
    rough = sb.inputs["Roughness"].default_value if sb else 0.8
    name = slot[len(piece) + 1:]
    if name in tile:
        geo = nt.nodes.new("ShaderNodeNewGeometry")
        mapping = nt.nodes.new("ShaderNodeMapping")
        mapping.inputs["Scale"].default_value = (0.5, 0.5, 0.5)
        nt.links.new(geo.outputs["Position"], mapping.inputs["Vector"])
        img = nt.nodes.new("ShaderNodeTexImage")
        img.image = plaster
        img.projection = "BOX"
        img.projection_blend = 0.2
        nt.links.new(mapping.outputs["Vector"], img.inputs["Vector"])
        tint = nt.nodes.new("ShaderNodeMix")
        tint.data_type = "RGBA"
        tint.blend_type = "MULTIPLY"
        tint.inputs["Factor"].default_value = 1.0
        nt.links.new(img.outputs["Color"], tint.inputs[6])
        tint.inputs[7].default_value = (*tile[name], 1.0)
        base = tint.outputs[2]
    else:
        rgb = nt.nodes.new("ShaderNodeRGB")
        rgb.outputs[0].default_value = tuple(sb.inputs["Base Color"].default_value) if sb else (0.5, 0.5, 0.5, 1)
        base = rgb.outputs[0]
    uvn = nt.nodes.new("ShaderNodeUVMap")
    uvn.uv_map = "UVMap"
    mk = nt.nodes.new("ShaderNodeTexImage")
    mk.image = masks_img
    nt.links.new(uvn.outputs["UV"], mk.inputs["Vector"])
    alpha = nt.nodes.new("ShaderNodeMapRange")
    alpha.inputs["To Min"].default_value = 0.4
    nt.links.new(mk.outputs["Alpha"], alpha.inputs["Value"])
    ao = nt.nodes.new("ShaderNodeMix")
    ao.data_type = "RGBA"
    ao.blend_type = "MULTIPLY"
    ao.inputs["Factor"].default_value = 1.0
    nt.links.new(base, ao.inputs[6])
    nt.links.new(alpha.outputs["Result"], ao.inputs[7])
    nt.links.new(ao.outputs[2], bsdf.inputs["Base Color"])
    bsdf.inputs["Roughness"].default_value = rough
    sep = nt.nodes.new("ShaderNodeSeparateColor")
    nt.links.new(mk.outputs["Color"], sep.inputs["Color"])
    em = nt.nodes.new("ShaderNodeMix")
    em.data_type = "RGBA"
    em.blend_type = "MULTIPLY"
    em.inputs["Factor"].default_value = 1.0
    nt.links.new(base, em.inputs[6])
    nt.links.new(sep.outputs["Blue"], em.inputs[7])
    nt.links.new(em.outputs[2], bsdf.inputs["Emission Color"])
    bsdf.inputs["Emission Strength"].default_value = 0.0
    return mat, bsdf


def place(piece, glb, masks_png, tile, at=(0.0, 0.0, 0.0), show=True):
    before = set(bpy.data.objects)
    bpy.ops.import_scene.gltf(filepath=str(glb))
    new = [o for o in bpy.data.objects if o not in before]
    masks_img = masks_image(masks_png)
    emit = []
    for o in new:
        if o.parent is None:
            o.location = at
        if o.type != "MESH":
            continue
        o.hide_render = not show or f"_lod{lod}" not in o.name
        for s in o.material_slots:
            if s.material:
                mat, bsdf = material(piece, s.material.name.split(".")[0], s.material, masks_img, tile)
                s.material = mat
                emit.append(bsdf)
    return new, emit


def committed_tile(piece):
    return {k[len(piece) + 1:]: tuple(v["color"]) for k, v in PIECES[piece]["slots"].items() if "tileable" in v}


own, emitters = place(P, STAGE / f"{P}.glb", STAGE / f"{P}.masks.png", OWN_TILE)
others = []
if lod == 0:
    for pid, at in (("house_cottage", (0.0, 9.0, 0.0)), ("house_longhouse", (0.0, -9.0, 0.0))):
        objs, _ = place(pid, KIT / PIECES[pid]["glb"], KIT / (PIECES[pid]["maps"]["masks"] + ".png"),
                        committed_tile(pid), at, show=False)
        others += [o for o in objs if o.type == "MESH" and "_lod0" in o.name]

ground = bpy.data.objects.new("ground", bpy.data.meshes.new("ground"))
ground.data.from_pydata([(-24, -24, -0.02), (24, -24, -0.02), (24, 24, -0.02), (-24, 24, -0.02)], [], [(0, 1, 2, 3)])
gm = bpy.data.materials.new("ground")
gm.node_tree.nodes["Principled BSDF"].inputs["Base Color"].default_value = (0.09, 0.16, 0.05, 1.0)
ground.data.materials.append(gm)
scn.collection.objects.link(ground)


def camera(name, loc, look_at, lens=40.0, ortho=None):
    cam = bpy.data.objects.new(name, bpy.data.cameras.new(name))
    cam.data.lens = lens
    if ortho:
        cam.data.type = "ORTHO"
        cam.data.ortho_scale = ortho
    cam.location = loc
    d = [look_at[i] - loc[i] for i in range(3)]
    cam.rotation_euler = (math.atan2(math.hypot(d[0], d[1]), -d[2]), 0.0, math.atan2(d[1], d[0]) - math.pi / 2)
    scn.collection.objects.link(cam)
    return cam


def shoot(cam, name):
    scn.camera = cam
    scn.render.filepath = str(out / f"{name}.jpg")
    bpy.ops.render.render(write_still=True)


sfx = "" if lod == 0 else f"-lod{lod}"
shoot(camera("c34", (13.0, 9.0, 8.5), (0.0, 0.2, 1.9), 40.0), f"eevee-three-quarter{sfx}")
shoot(camera("cf", (13.0, 0.0, 2.2), (0.0, 0.0, 2.2), 38.0), f"eevee-front{sfx}")
shoot(camera("cb", (-11.0, -9.0, 7.0), (0.0, 0.0, 2.0), 40.0), f"eevee-back{sfx}")
if lod == 0:
    shoot(camera("ct", (0.0, 0.01, 30.0), (0.0, 0.0, 0.0), 40.0, ortho=9.5), "eevee-top")
    for o in others:
        o.hide_render = False
    shoot(camera("ctrio", (26.0, 3.0, 13.0), (0.0, 0.0, 2.5), 32.0), "eevee-trio")
    for o in others:
        o.hide_render = True
    sun.data.energy = 0.0
    bg.inputs[1].default_value = 0.05
    for b in emitters:
        b.inputs["Emission Strength"].default_value = 4.0
    shoot(camera("cn", (10.0, -4.0, 2.6), (1.5, 0.0, 1.6), 40.0), "eevee-night")
